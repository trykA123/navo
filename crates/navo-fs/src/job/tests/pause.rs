use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::thread::{Scope, ScopedJoinHandle, scope};
use std::time::Duration;

use navo_core::Totals;

use crate::run_job;

use super::*;

const WAIT: Duration = Duration::from_secs(5);
const CHUNK: u64 = 4096;

struct Shared(Arc<Mutex<Log>>);

impl JobLog for Shared {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.0.lock().expect("log").step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.0.lock().expect("log").undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.0.lock().expect("log").journal_held(held)
    }
}

struct Stop(CancelToken);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) struct Running<'scope> {
    pub(super) token: CancelToken,
    _stop: Stop,
    log: Arc<Mutex<Log>>,
    pub(super) seen: Receiver<Progress>,
    job: ScopedJoinHandle<'scope, JobOutcome>,
}

impl Running<'_> {
    pub(super) fn next_pause(&self) -> Option<Progress> {
        loop {
            let progress = self.seen.recv_timeout(WAIT).ok()?;
            if progress.paused {
                return Some(progress);
            }
        }
    }

    pub(super) fn ends_promptly(&self) -> bool {
        loop {
            match self.seen.recv_timeout(WAIT) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }

    pub(super) fn logged(&self) -> Vec<(u32, StepOutcome)> {
        self.log.lock().expect("log").entries.clone()
    }

    pub(super) fn done(&self) -> Vec<u32> {
        self.log.lock().expect("log").done()
    }

    pub(super) fn finish(self) -> JobOutcome {
        let has_ended = self.ends_promptly();
        self.token.cancel();
        let outcome = self.job.join().expect("job thread");
        assert!(has_ended, "the job did not end");
        outcome
    }
}

fn start<'scope>(
    scope: &'scope Scope<'scope, '_>,
    ops: &'scope dyn ProviderOps,
    plan: &'scope OperationPlan,
    pause_when: impl FnMut(&Progress) -> bool + Send + 'scope,
) -> Running<'scope> {
    let job = Paused {
        ops,
        plan,
        workers: 0,
    };
    start_with(scope, job, pause_when)
}

pub(super) struct Paused<'scope> {
    pub(super) ops: &'scope dyn ProviderOps,
    pub(super) plan: &'scope OperationPlan,
    pub(super) workers: usize,
}

pub(super) fn start_with<'scope>(
    scope: &'scope Scope<'scope, '_>,
    Paused { ops, plan, workers }: Paused<'scope>,
    mut pause_when: impl FnMut(&Progress) -> bool + Send + 'scope,
) -> Running<'scope> {
    let token = CancelToken::new();
    let log = Arc::new(Mutex::new(Log::default()));
    let (sender, seen) = channel();
    let (cancel, shared) = (token.clone(), Arc::clone(&log));
    let job = scope.spawn(move || {
        let mut last = None;
        let mut on_progress = |progress: &Progress| {
            let at = (progress.bytes_done, progress.files_done);
            if !progress.paused && last != Some(at) && pause_when(progress) {
                last = Some(at);
                cancel.pause();
            }
            let _ = sender.send(progress.clone());
        };
        let mut ctx = JobContext {
            ops,
            log: &mut Shared(shared),
            cancel: &cancel,
            progress: &mut on_progress,
        };
        match workers {
            0 => run_job(plan, &[], &mut ctx),
            n => super::super::parallel::run(plan, &[], &mut ctx, n),
        }
    });
    Running {
        _stop: Stop(token.clone()),
        token,
        log,
        seen,
        job,
    }
}

pub(super) fn clean_copy() -> Snapshot {
    let fs = three_files();
    let _ = run(&fs, &copy_three(&fs));
    tree_of(&fs, "")
}

fn at_bytes(bytes: u64) -> impl FnMut(&Progress) -> bool + Send {
    move |progress| progress.bytes_done == bytes
}

#[test]
fn pause_stops_progress_within_one_chunk() {
    let fs = three_files();
    let plan = copy_three(&fs);
    scope(|s| {
        let job = start(s, &fs, &plan, at_bytes(CHUNK));
        let paused = job.next_pause().expect("the job pauses");
        assert_eq!(
            (
                paused.bytes_done,
                size(&fs, &temp("dst/one.txt")),
                exists(&fs, "dst/one.txt")
            ),
            (CHUNK, Some(CHUNK), false)
        );
        job.token.cancel();
        let _ = job.finish();
    });
}

#[test]
fn resume_continues_to_the_same_result_as_a_clean_run() {
    let fs = three_files();
    let plan = copy_three(&fs);
    scope(|s| {
        let job = start(s, &fs, &plan, at_bytes(CHUNK));
        let _ = job.next_pause().expect("the job pauses");
        job.token.resume();
        let after = job.seen.recv_timeout(WAIT).expect("a report after resume");
        assert!(!after.paused);
        assert_eq!(job.finish(), JobOutcome::Done);
    });
    assert_eq!(
        (tree_of(&fs, ""), leftovers(&fs)),
        (clean_copy(), Vec::new())
    );
}

#[test]
fn cancel_while_paused_ends_promptly_like_any_cancel() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let logged = scope(|s| {
        let job = start(s, &fs, &plan, at_bytes(BIG + CHUNK));
        let _ = job.next_pause().expect("the job pauses");
        job.token.cancel();
        assert!(job.ends_promptly());
        let logged = job.logged();
        assert_eq!(job.finish(), JobOutcome::Cancelled);
        logged
    });
    let plain = three_files();
    let mut harness = Harness {
        cancel_after_bytes: Some(BIG),
        ..Harness::default()
    };
    let _ = harness.run(&plain, &copy_three(&plain), &[]);
    assert_eq!(
        (tree_of(&fs, ""), logged),
        (tree_of(&plain, ""), harness.logged())
    );
    assert_eq!(leftovers(&fs), Vec::<String>::new());
}

#[test]
fn a_pause_at_a_file_boundary_holds_before_the_next_file() {
    let fs = three_files();
    let plan = copy_three(&fs);
    scope(|s| {
        let job = start(s, &fs, &plan, |p: &Progress| {
            (p.files_done, p.bytes_done) == (1, BIG)
        });
        let paused = job.next_pause().expect("the job pauses");
        assert_eq!((paused.files_done, paused.bytes_done), (1, BIG));
        assert_eq!(job.logged(), [(0, StepOutcome::Done)]);
        assert_eq!(fs.calls("src/two.txt", FakeOp::Copy), 0);
        assert_eq!(
            tree_of(&fs, "dst"),
            [entry("one.txt", EntryKind::File, BIG)]
        );
        job.token.resume();
        assert_eq!(job.finish(), JobOutcome::Done);
    });
    assert_eq!(tree_of(&fs, ""), clean_copy());
}

#[test]
fn pause_and_resume_many_times_still_copies_everything() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let pauses = scope(|s| {
        let job = start(s, &fs, &plan, |_: &Progress| true);
        let mut pauses = 0;
        while job.next_pause().is_some() {
            pauses += 1;
            job.token.resume();
        }
        assert_eq!(job.finish(), JobOutcome::Done);
        pauses
    });
    assert!(pauses >= 8, "{pauses}");
    assert_eq!(
        (tree_of(&fs, ""), leftovers(&fs)),
        (clean_copy(), Vec::new())
    );
}

#[test]
fn a_crash_while_paused_rolls_back_cleanly() {
    let fs = three_files();
    let before = tree_of(&fs, "");
    let plan = copy_three(&fs);
    scope(|s| {
        let job = start(s, &fs, &plan, at_bytes(BIG + CHUNK));
        let _ = job.next_pause().expect("the job pauses");
        let done = job.done();
        assert_eq!(done, [0]);
        assert_eq!(clean_up(&plan, &done, &fs), Ok(1));
        let mut next_start = Harness::default();
        assert_eq!(next_start.roll_back(&fs, &plan, &done), JobOutcome::Done);
        assert_eq!(
            (tree_of(&fs, ""), leftovers(&fs)),
            (before.clone(), Vec::new())
        );
        job.token.cancel();
        let _ = job.finish();
    });
}

#[test]
fn a_delete_pauses_between_files() {
    let fs = three_files();
    let plan = plan(&PlanRequest {
        target: None,
        existing: Vec::new(),
        ..request(&fs, OpKind::Delete, &["src/one.txt", "src/two.txt"], "dst")
    });
    scope(|s| {
        let job = start(s, &fs, &plan, |p: &Progress| p.files_done == 1);
        let _ = job.next_pause().expect("the job pauses");
        assert_eq!(job.logged().len(), 1);
        assert_eq!(tree_of(&fs, "src").len(), 2);
        job.token.resume();
        assert_eq!(job.finish(), JobOutcome::Done);
    });
    assert_eq!(
        tree_of(&fs, "src"),
        [entry("three.txt", EntryKind::File, 3_000)]
    );
}

#[test]
fn cancel_while_paused_after_the_last_chunk_never_places_the_file() {
    let fs = three_files();
    let plan = copy_three(&fs);
    scope(|s| {
        let job = start(s, &fs, &plan, |p: &Progress| {
            (p.bytes_done, p.files_done) == (BIG, 0)
        });
        let _ = job.next_pause().expect("the job pauses");
        assert_eq!(size(&fs, &temp("dst/one.txt")), Some(BIG));
        job.token.cancel();
        assert_eq!(job.finish(), JobOutcome::Cancelled);
    });
    assert_eq!(
        (tree_of(&fs, "dst"), leftovers(&fs)),
        (Snapshot::new(), Vec::new())
    );
}

struct RetakeFail<'a> {
    fs: &'a FakeFs,
    cancel: &'a CancelToken,
    pauses: AtomicUsize,
}

impl Provider for RetakeFail<'_> {
    fn id(&self) -> navo_core::ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> crate::Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn crate::ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<crate::FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for RetakeFail<'_> {
    fn pause_git(&self, location: &Location) -> Result<Option<crate::GitPauseGuard>, FsError> {
        if self
            .pauses
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            self.fs.pause_git(location)
        } else {
            Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::Locked)
            })
        }
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        let result = self.fs.remove_file(location);
        self.cancel.pause();
        result
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

#[expect(clippy::too_many_lines, reason = "one pause and retake scenario")]
#[test]
fn a_failed_retake_reports_the_next_pending_seq() {
    let fs = FakeFs::new();
    fs.add_file("dir/a", 1);
    fs.add_file("dir/b", 1);
    let plan = OperationPlan {
        kind: OpKind::Delete,
        steps: vec![
            Step::RemoveFile {
                location: fs.location("dir/a"),
            },
            Step::RemoveFile {
                location: fs.location("dir/b"),
            },
            Step::RemoveFolder {
                location: fs.location("dir"),
            },
        ],
        totals: Totals {
            files: 2,
            folders: 0,
            bytes: 2,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    let cancel = CancelToken::new();
    let ops = RetakeFail {
        fs: &fs,
        cancel: &cancel,
        pauses: AtomicUsize::new(0),
    };
    let (sender, paused) = channel();
    let token = cancel.clone();
    let outcome = std::thread::scope(|scope| {
        let job = scope.spawn(move || {
            let mut log = Log::default();
            let mut progress = |progress: &Progress| {
                let _ = progress.paused.then(|| sender.send(()));
            };
            let mut ctx = JobContext {
                ops: &ops,
                log: &mut log,
                cancel: &token,
                progress: &mut progress,
            };
            run_job(&plan, &[], &mut ctx)
        });
        paused.recv_timeout(WAIT).expect("the job pauses");
        cancel.resume();
        job.join().unwrap()
    });
    assert!(
        matches!(outcome, JobOutcome::Failed { seq: 1, .. }),
        "{outcome:?}"
    );
    assert!(fs.metadata(&fs.location("dir/a")).is_err());
    assert!(fs.metadata(&fs.location("dir/b")).is_ok());
}

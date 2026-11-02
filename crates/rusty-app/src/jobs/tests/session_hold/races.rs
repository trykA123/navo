use std::sync::{Mutex, mpsc};
use std::time::Duration;

use rusty_core::{FsError, ProviderId};
use rusty_fs::{Capabilities, FileIdentity, ListSink};
use rusty_ui::browser::OpLog;

use super::*;

enum PauseAt {
    SaveFlush,
    RestoreHash,
}

struct PausedSave {
    fs: Arc<FakeFs>,
    paused: mpsc::Sender<()>,
    resume: Mutex<Option<mpsc::Receiver<()>>>,
    at: PauseAt,
}

impl PausedSave {
    fn pause(&self) {
        let resume = self.resume.lock().unwrap().take();
        if let Some(resume) = resume {
            self.paused.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }
}

impl Provider for PausedSave {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        at: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(at, sink, cancel)
    }

    fn metadata(&self, at: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(at)
    }
}

impl ProviderOps for PausedSave {
    fn create_folder(&self, at: &Location) -> Result<(), FsError> {
        self.fs.create_folder(at)
    }

    fn create_file(&self, at: &Location) -> Result<(), FsError> {
        self.fs.create_file(at)
    }

    fn copy_file(
        &self,
        from: &Location,
        to: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.fs.copy_file(from, to, progress, cancel)
    }

    fn rename(&self, from: &Location, to: &Location) -> Result<(), FsError> {
        self.fs.rename(from, to)
    }

    fn replace(&self, from: &Location, to: &Location) -> Result<(), FsError> {
        self.fs.replace(from, to)
    }

    fn remove_file(&self, at: &Location) -> Result<(), FsError> {
        self.fs.remove_file(at)
    }

    fn remove_link(&self, at: &Location) -> Result<(), FsError> {
        self.fs.remove_link(at)
    }

    fn remove_empty_folder(&self, at: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(at)
    }

    fn recycle(&self, at: &Location) -> Result<(), FsError> {
        self.fs.recycle(at)
    }

    fn file_identity(&self, at: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.fs.file_identity(at)
    }

    fn volume_serial(&self, at: &Location) -> Result<Option<u64>, FsError> {
        self.fs.volume_serial(at)
    }

    fn mount_identity(&self, at: &Location) -> Result<Option<u64>, FsError> {
        self.fs.mount_identity(at)
    }

    fn content_hash(&self, at: &Location, cancel: &CancelToken) -> Result<[u8; 32], FsError> {
        if matches!(self.at, PauseAt::RestoreHash) {
            self.pause();
        }
        self.fs.content_hash(at, cancel)
    }

    fn flush(&self, at: &Location) -> Result<(), FsError> {
        if matches!(self.at, PauseAt::SaveFlush)
            && at == &self.fs.location("dest")
            && self.fs.metadata(&self.fs.location(TARGET)).is_err()
        {
            self.pause();
        }
        self.fs.flush(at)
    }
}

fn run_checked(log: &mut dyn OpLog, plan: &OperationPlan, ops: &dyn ProviderOps) {
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan,
        ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops,
            log,
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, ops).unwrap();
}

fn restore_reporting(
    session: &Session,
    report: mpsc::Sender<bool>,
) -> std::thread::JoinHandle<Result<PreviousVersion, RestoreError>> {
    let holds = session.holds.clone();
    let ops = session.ops();
    std::thread::spawn(move || {
        let result = holds.restore_waiting(
            SAVE,
            &ops,
            crate::jobs::mutation_lock::Wait {
                waiting: Some(&mut |is_waiting| report.send(is_waiting).unwrap()),
                ..crate::jobs::mutation_lock::Wait::bounded()
            },
        );
        report.send(false).unwrap();
        result
    })
}

#[test]
fn restore_waits_for_a_default_save_paused_after_removing_the_target() {
    let session = Session::new(OFF);
    session.save();
    session.put(STAGED, (17, 3_000), [3; 32]);
    let plan = write_plan(
        &session.at(TARGET),
        &session.at(STAGED),
        FileStamp {
            size: 11,
            modified_ms: Some(2_000),
            hash: NEW_HASH,
        },
        17,
    );
    let mut log = session.holds.factory(None)(&plan).unwrap();
    let (paused, seen) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let fs = PausedSave {
        fs: session.fs.clone(),
        paused,
        resume: Mutex::new(Some(resumed)),
        at: PauseAt::SaveFlush,
    };
    let worker = std::thread::spawn(move || run_checked(log.as_mut(), &plan, &fs));
    seen.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(session.file(TARGET), None);
    let (report, waiting) = mpsc::channel();
    let restore = restore_reporting(&session, report);
    let reported = waiting.recv_timeout(Duration::from_secs(10));
    let target_while_waiting = session.file(TARGET);
    resume.send(()).unwrap();
    let saved = worker.join();
    let restored = restore.join();
    assert_eq!(reported, Ok(true));
    assert_eq!(target_while_waiting, None);
    saved.unwrap();
    assert_eq!(restored.unwrap(), Err(RestoreError::Changed));
    assert_eq!(session.file(TARGET), Some((17, Some(3_000), [3; 32])));
    assert!(session.kept_old());
    assert_eq!(session.state(), JobState::Done);
    let old = session.held();
    session.holds.release(session.fs.as_ref());
    assert!(old.iter().all(|at| session.present(at).is_none()));
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
}

#[test]
fn a_default_recycle_passes_check_while_a_default_copy_is_running() {
    let session = Session::new(OFF);
    session.fs.add_file("home/docs/a", 7);
    session.fs.add_file("recycle", 3);
    session.fs.add_folder("dest");
    let copy = copy_plan(&session.fs);
    let mut copying = session.holds.factory(None)(&copy).unwrap();
    copying
        .check(LoggedOperation {
            plan: &copy,
            ops: session.fs.as_ref(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            waiting: &mut |_| {},
            retain_deletes: None,
        })
        .unwrap();
    let mut recycle = copy.clone();
    recycle.kind = OpKind::Delete;
    recycle.steps = vec![rusty_core::Step::Recycle {
        location: session.at("recycle"),
    }];
    let mut deleting = session.holds.factory(None)(&recycle).unwrap();
    let fs = session.fs.clone();
    let worker = std::thread::spawn(move || {
        let cancel = CancelToken::new();
        deleting.check(LoggedOperation {
            plan: &recycle,
            ops: fs.as_ref(),
            cancel: &cancel,
            progress: &mut |_| {},
            waiting: &mut |waiting| {
                if waiting {
                    cancel.cancel();
                }
            },
            retain_deletes: None,
        })
    });
    assert!(worker.join().unwrap().is_ok());
    drop(copying);
}

#[test]
fn default_restore_waits_for_writers_without_using_the_experimental_lease() {
    for new_file in [false, true] {
        let session = Session::new(OFF);
        session.save();
        let exclusive = crate::jobs::test_mutation_file(&session.dirs.data);
        exclusive.try_lock().unwrap();
        let target = session.at(TARGET);
        let writer: Box<dyn Send> = if new_file {
            crate::jobs::write_factory(session.dirs.clone(), false)(&target).unwrap()
        } else {
            Box::new(crate::jobs::claim_write(&session.dirs, &target, false).unwrap())
        };
        let (report, waiting) = mpsc::channel();
        let restore = restore_reporting(&session, report);
        let reported = waiting.recv_timeout(Duration::from_secs(10));
        let target_while_waiting = session.file(TARGET);
        drop(writer);
        let restored = restore.join().unwrap();
        assert_eq!(reported, Ok(true));
        assert_eq!(target_while_waiting, SAVED);
        assert_eq!(
            restored,
            Ok(PreviousVersion::Undone {
                at: session.at(TARGET)
            })
        );
        assert_eq!(session.file(TARGET), ORIGINAL);
    }
}

#[test]
fn restore_excludes_new_default_jobs_until_undo_finishes() {
    let session = Session::new(OFF);
    session.save();
    session.fs.add_file("home/docs/a", 7);
    session.fs.add_folder("dest");
    let plan = copy_plan(&session.fs);
    let (paused, seen) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let ops: Arc<dyn ProviderOps> = Arc::new(PausedSave {
        fs: session.fs.clone(),
        paused,
        resume: Mutex::new(Some(resumed)),
        at: PauseAt::RestoreHash,
    });
    let holds = session.holds.clone();
    let restore = std::thread::spawn(move || holds.restore_previous(SAVE, &ops));
    seen.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut results = Vec::new();
    for factory in [
        session.holds.factory(None),
        profile_factory(session.journal.clone(), session.dirs.clone()),
        crate::jobs::guard_factory(Some(session.dirs.clone())),
    ] {
        let mut log = factory(&plan).unwrap();
        let cancel = CancelToken::new();
        let mut reports = Vec::new();
        let result = log.check(LoggedOperation {
            plan: &plan,
            ops: session.fs.as_ref(),
            cancel: &cancel,
            progress: &mut |_| {},
            waiting: &mut |waiting| {
                reports.push(waiting);
                if waiting {
                    cancel.cancel();
                }
            },
            retain_deletes: None,
        });
        results.push((result, reports));
    }
    resume.send(()).unwrap();
    let restored = restore.join().unwrap();
    for (result, reports) in results {
        assert_eq!(result, Err(rusty_l10n::tr("job-wait-cancelled")));
        assert_eq!(reports, [true, false]);
    }
    assert!(restored.is_ok());
    assert_eq!(session.file(TARGET), ORIGINAL);
}

#[test]
fn restore_hashing_does_not_block_releasing_session_holds() {
    for copy in [false, true] {
        let session = Session::new(OFF);
        let (target, original) = if copy {
            copies::copy_with(&session, &session.holds.compare_factory(None));
            ("dest/a", Some((2, Some(1_000), OLD_HASH)))
        } else {
            session.save();
            (TARGET, ORIGINAL)
        };
        let (paused, seen) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let ops: Arc<dyn ProviderOps> = Arc::new(PausedSave {
            fs: session.fs.clone(),
            paused,
            resume: Mutex::new(Some(resumed)),
            at: PauseAt::RestoreHash,
        });
        let holds = session.holds.clone();
        let restore = std::thread::spawn(move || holds.restore_previous(SAVE, &ops));
        seen.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(session.state(), JobState::RestorePending);
        let holds = session.holds.clone();
        let fs = session.fs.clone();
        let (done, released) = mpsc::channel();
        let release = std::thread::spawn(move || {
            holds.release(fs.as_ref());
            done.send(()).unwrap();
        });
        let result = released.recv_timeout(Duration::from_secs(5));
        resume.send(()).unwrap();
        let restored = restore.join().unwrap();
        release.join().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(
            restored,
            Ok(PreviousVersion::Undone {
                at: session.at(target)
            })
        );
        assert_eq!(session.file(target), original);
    }
}

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use rusty_core::{Location, ProviderId};

use super::crashing::Crashing;
use super::*;
use crate::{Capabilities, ListSink};

const WAIT: Duration = Duration::from_secs(5);

pub(super) fn tree(folders: usize, per_folder: usize) -> FakeFs {
    let fs = fixture(&[]);
    for folder in 0..folders {
        for file in 0..per_folder {
            fs.add_file(&format!("src/d{}/e{folder}/f{file}.txt", folder % 3), 1);
        }
    }
    fs
}

pub(super) fn delete_all(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        target: None,
        existing: Vec::new(),
        ..request(fs, OpKind::Delete, &["src"], "dst")
    })
}

pub(super) fn location_of(step: &Step) -> &Location {
    match step {
        Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => location,
        other => panic!("a delete plan holds only removals, not {other:?}"),
    }
}

fn inside(child: &Location, folder: &Location) -> bool {
    std::iter::successors(child.parent(), Location::parent).any(|up| up.same_place(folder))
}

#[derive(Default)]
struct Watch {
    state: Mutex<(Vec<Location>, usize)>,
    wake: Condvar,
    early: Mutex<Vec<Location>>,
}

struct Watched<'a> {
    fs: &'a FakeFs,
    watch: &'a Watch,
    want: usize,
    deadline: Instant,
}

impl Watched<'_> {
    fn removing(
        &self,
        location: &Location,
        remove: impl FnOnce() -> Result<(), FsError>,
    ) -> Result<(), FsError> {
        let mut state = self.watch.state.lock().expect("watch");
        state.0.push(location.clone());
        state.1 = state.1.max(state.0.len());
        self.watch.wake.notify_all();
        let want = self.want;
        let state = self.watch.wake.wait_timeout_while(
            state,
            self.deadline.saturating_duration_since(Instant::now()),
            |state| state.1 < want,
        );
        drop(state);
        let result = remove();
        let mut state = self.watch.state.lock().expect("watch");
        state.0.retain(|busy| busy != location);
        result
    }
}

impl Provider for Watched<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for Watched<'_> {
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
        self.removing(location, || self.fs.remove_file(location))
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        self.fs.make_link(target_of, at)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.removing(location, || self.fs.remove_link(location))
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        let state = self.watch.state.lock().expect("watch");
        if state.0.iter().any(|busy| inside(busy, location)) {
            self.watch
                .early
                .lock()
                .expect("early")
                .push(location.clone());
        }
        drop(state);
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

fn watched_delete(workers: usize, want: usize) -> (JobOutcome, usize, Vec<Location>, FakeFs) {
    let fs = tree(12, 40);
    add_link(&fs, "src/d0/e0", "shortcut");
    let plan = delete_all(&fs);
    let watch = Watch::default();
    let ops = Watched {
        fs: &fs,
        watch: &watch,
        want,
        deadline: Instant::now() + WAIT,
    };
    let outcome = with_workers(workers).run(&ops, &plan, &[]);
    let peak = watch.state.lock().expect("watch").1;
    let early = watch.early.lock().expect("early").clone();
    (outcome, peak, early, fs)
}

fn with_workers(workers: usize) -> Harness {
    Harness {
        workers,
        ..Harness::default()
    }
}

#[test]
fn a_parallel_delete_removes_files_with_more_than_one_worker() {
    let (outcome, peak, _, fs) = watched_delete(4, 4);
    assert_eq!(
        (outcome, peak, exists(&fs, "src")),
        (JobOutcome::Done, 4, false)
    );
}

#[test]
fn a_parallel_delete_removes_a_folder_only_after_everything_inside_it() {
    let (outcome, _, early, _) = watched_delete(8, 2);
    assert_eq!((outcome, early), (JobOutcome::Done, Vec::new()));
}

#[test]
fn a_parallel_move_still_removes_sources_one_at_a_time_after_the_copies() {
    let fs = tree(3, 5);
    let plan = plan(&request(&fs, OpKind::Move, &["src/d0"], "dst"));
    let watch = Watch::default();
    let ops = Watched {
        fs: &fs,
        watch: &watch,
        want: 2,
        deadline: Instant::now() + Duration::from_millis(500),
    };
    let mut harness = with_workers(4);
    assert_eq!(harness.run(&ops, &plan, &[]), JobOutcome::Done);
    let peak = watch.state.lock().expect("watch").1;
    assert_eq!(
        (peak, exists(&fs, "src/d0"), exists(&fs, "dst/d0/e0/f4.txt")),
        (1, false, true)
    );
}

#[test]
fn a_ten_thousand_file_delete_writes_the_journal_at_most_thirty_times() {
    for workers in [0, 4] {
        let fs = tree(50, 200);
        let plan = delete_all(&fs);
        let mut harness = with_workers(workers);
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        assert_eq!(harness.log.entries.len(), plan.steps.len(), "{workers}");
        assert!(
            harness.log.writes <= 30,
            "{workers}: {} writes",
            harness.log.writes
        );
    }
}

#[test]
fn a_copy_still_journals_every_step_on_its_own() {
    let fs = tree(2, 30);
    let plan = copy_plan(&fs, &["src/d0"], "dst");
    let mut harness = with_workers(4);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    assert_eq!(harness.log.writes, plan.steps.len());
}

pub(super) fn removed(fs: &FakeFs, plan: &OperationPlan) -> Vec<u32> {
    (0_u32..)
        .zip(&plan.steps)
        .filter(|(_, step)| fs.metadata(location_of(step)).is_err())
        .map(|(seq, _)| seq)
        .collect()
}

fn marked(log: &Log) -> Vec<u32> {
    let mut seqs: Vec<u32> = log
        .entries
        .iter()
        .filter(|(_, outcome)| *outcome != StepOutcome::Failed)
        .map(|(seq, _)| *seq)
        .collect();
    seqs.sort_unstable();
    seqs
}

#[test]
fn a_cancelled_delete_journals_exactly_the_removed_entries() {
    for workers in [0, 4] {
        let fs = tree(20, 100);
        let plan = delete_all(&fs);
        let mut harness = Harness {
            workers,
            ..Harness::default()
        };
        let cancel = harness.cancel.clone();
        let mut ctx_progress = |progress: &Progress| {
            if progress.files_done >= 700 {
                cancel.cancel();
            }
        };
        let outcome = {
            let mut ctx = JobContext {
                ops: &fs,
                log: &mut harness.log,
                cancel: &harness.cancel,
                progress: &mut ctx_progress,
            };
            run_job_parallel(&plan, &[], &mut ctx, workers.max(1))
        };
        let gone = removed(&fs, &plan);
        assert_eq!(outcome, JobOutcome::Cancelled, "{workers}");
        assert!(
            gone.len() >= 700 && gone.len() < plan.steps.len(),
            "{workers}"
        );
        assert_eq!(marked(&harness.log), gone, "{workers}");
    }
}

struct UntilDeath<'a> {
    log: Log,
    ops: &'a Crashing<'a>,
}

impl JobLog for UntilDeath<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if self.ops.is_dead() {
            return Ok(());
        }
        self.log.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.log.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.log.journal_held(held)
    }
}

fn crashed_delete(workers: usize) -> (FakeFs, OperationPlan, Vec<u32>) {
    let fs = tree(20, 100);
    let plan = delete_all(&fs);
    let ops = Crashing::new(&fs, "src/d1/e10/f50.txt", FakeOp::Remove);
    let mut log = UntilDeath {
        log: Log::default(),
        ops: &ops,
    };
    let outcome = run_job_parallel(
        &plan,
        &[],
        &mut JobContext {
            ops: &ops,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        workers,
    );
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    let done = log.log.done();
    (fs, plan, done)
}

#[test]
fn a_delete_that_crashed_with_unjournalled_removals_resumes_to_the_end() {
    for workers in [1, 4] {
        let (fs, plan, done) = crashed_delete(workers);
        let unmarked = removed(&fs, &plan)
            .into_iter()
            .filter(|seq| !done.contains(seq))
            .count();
        assert!(unmarked > 0, "{workers}: the crash lost no marks");
        let mut harness = with_workers(workers);
        let outcome = harness.run(&fs, &plan, &done);
        assert_eq!(
            (outcome, exists(&fs, "src")),
            (JobOutcome::Done, false),
            "{workers}"
        );
    }
}

#[test]
fn a_delete_that_crashed_with_unjournalled_removals_rolls_back_without_touching_files() {
    let (fs, plan, done) = crashed_delete(4);
    let before = tree_of(&fs, "");
    let mut next_start = Harness::default();
    assert_eq!(next_start.roll_back(&fs, &plan, &done), JobOutcome::Done);
    assert_eq!(tree_of(&fs, ""), before);
}

#[test]
fn a_delete_whose_drive_vanished_fails_instead_of_skipping() {
    let fs = tree(2, 3);
    let plan = delete_all(&fs);
    unplug(&fs, "src");
    let (outcome, _) = run_in(4, &fs, &plan);
    assert_eq!(
        failure(&outcome).map(|(_, kind)| kind),
        Some(FsErrorKind::NotFound)
    );
}

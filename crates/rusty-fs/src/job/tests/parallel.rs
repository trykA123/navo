use std::sync::{Condvar, Mutex};
use std::time::Duration;

use rusty_core::{Location, ProviderId};

use super::crashing::crashed_with;
use super::*;
use crate::{Capabilities, ListSink};

const WORKERS: [usize; 3] = [1, 4, 8];
const WAIT: Duration = Duration::from_secs(5);

fn many_files() -> FakeFs {
    let mut files = vec![("src/top.txt", 2_500)];
    files.extend([
        ("src/a/a0.txt", BIG),
        ("src/a/a1.txt", 6_000),
        ("src/a/a2.txt", 0),
        ("src/a/b/b0.txt", 9_000),
        ("src/a/b/b1.txt", 4_096),
        ("src/a/b/c/c0.txt", 12_345),
        ("src/a/b/c/c1.txt", 1),
        ("src/a/b/c/c2.txt", 8_192),
        ("src/d/d0.txt", 7_777),
        ("src/d/d1.txt", BIG),
        ("src/d/e/e0.txt", 3_000),
    ]);
    let fs = fixture(&files);
    fs.add_folder("src/a/empty");
    fs
}

fn copy_many(fs: &FakeFs) -> OperationPlan {
    copy_plan(fs, &["src/a", "src/d", "src/top.txt"], "dst")
}

fn move_many(fs: &FakeFs) -> OperationPlan {
    plan(&request(
        fs,
        OpKind::Move,
        &["src/a", "src/d", "src/top.txt"],
        "dst",
    ))
}

fn flat(count: usize) -> FakeFs {
    let names: Vec<String> = (0..count).map(|i| format!("src/f{i}.txt")).collect();
    let files: Vec<(&str, u64)> = names.iter().map(|n| (n.as_str(), BIG)).collect();
    fixture(&files)
}

fn copy_flat(fs: &FakeFs, count: usize) -> OperationPlan {
    let names: Vec<String> = (0..count).map(|i| format!("src/f{i}.txt")).collect();
    let sources: Vec<&str> = names.iter().map(String::as_str).collect();
    copy_plan(fs, &sources, "dst")
}

fn with_workers(workers: usize) -> Harness {
    Harness {
        workers,
        ..Harness::default()
    }
}

fn sorted(mut entries: Vec<(u32, StepOutcome)>) -> Vec<(u32, StepOutcome)> {
    entries.sort_by_key(|(seq, _)| *seq);
    entries
}

type Ending = (JobOutcome, Snapshot, Vec<(u32, StepOutcome)>, Vec<String>);

fn ending(
    build: impl Fn() -> FakeFs,
    plan_of: impl Fn(&FakeFs) -> OperationPlan,
    workers: usize,
) -> Ending {
    let fs = build();
    let plan = plan_of(&fs);
    let mut harness = with_workers(workers);
    let outcome = harness.run(&fs, &plan, &[]);
    (
        outcome,
        tree_of(&fs, ""),
        sorted(harness.logged()),
        leftovers(&fs),
    )
}

fn ends_like_standard(build: impl Fn() -> FakeFs, plan_of: impl Fn(&FakeFs) -> OperationPlan) {
    let standard = ending(&build, &plan_of, 0);
    assert_eq!(standard.0, JobOutcome::Done);
    for workers in WORKERS {
        assert_eq!(
            ending(&build, &plan_of, workers),
            standard,
            "{workers} workers"
        );
    }
}

#[test]
fn a_parallel_copy_ends_like_standard_with_one_four_and_eight_workers() {
    ends_like_standard(many_files, copy_many);
}

#[test]
fn a_parallel_move_ends_like_standard() {
    ends_like_standard(many_files, move_many);
}

fn with_existing() -> FakeFs {
    let fs = many_files();
    fs.add_file("dst/a/a1.txt", 11);
    fs.add_file("dst/a/b/c/c0.txt", 22);
    fs.add_file("dst/top.txt", 33);
    fs
}

#[test]
fn parallel_overwrites_end_like_standard() {
    ends_like_standard(with_existing, |fs| {
        plan(&PlanRequest {
            policy: ConflictPolicy::OverwriteAll,
            ..request(fs, OpKind::Copy, &["src/a", "src/d", "src/top.txt"], "dst")
        })
    });
}

#[test]
fn a_parallel_move_that_skips_conflicts_keeps_those_sources_like_standard() {
    ends_like_standard(with_existing, |fs| {
        plan(&PlanRequest {
            policy: ConflictPolicy::Skip,
            ..request(fs, OpKind::Move, &["src/a", "src/d", "src/top.txt"], "dst")
        })
    });
}

#[test]
fn folders_exist_before_their_files_are_copied() {
    let (outcome, tree, _, _) = ending(many_files, copy_many, 8);
    assert_eq!(
        (
            outcome,
            tree.iter().any(|(path, _, _)| path == "dst/a/b/c/c2.txt")
        ),
        (JobOutcome::Done, true)
    );
}

#[test]
fn run_job_parallel_with_one_worker_reports_exactly_like_run_job() {
    let reports = |parallel: bool| {
        let fs = many_files();
        let plan = copy_many(&fs);
        let mut harness = Harness::default();
        let outcome = harness.with(&fs, |ctx| match parallel {
            true => run_job_parallel(&plan, &[], ctx, 1),
            false => run_job(&plan, &[], ctx),
        });
        (outcome, harness.progress, harness.log.entries)
    };
    assert_eq!(reports(true), reports(false));
}

fn parallel_progress() -> (OperationPlan, Vec<Progress>) {
    let fs = many_files();
    let plan = copy_many(&fs);
    let mut harness = with_workers(4);
    let _ = harness.run(&fs, &plan, &[]);
    (plan, harness.progress)
}

#[test]
fn parallel_progress_ends_at_the_totals() {
    let (plan, progress) = parallel_progress();
    let last = progress.last().expect("progress was reported");
    assert_eq!(
        (last.bytes_done, last.files_done),
        (plan.totals.bytes, plan.totals.files)
    );
}

#[test]
fn parallel_progress_never_goes_back() {
    let (_, progress) = parallel_progress();
    assert!(
        progress
            .windows(2)
            .all(|pair| pair[0].bytes_done <= pair[1].bytes_done
                && pair[0].files_done <= pair[1].files_done)
    );
}

#[test]
fn parallel_progress_names_the_most_recently_started_step() {
    let (plan, progress) = parallel_progress();
    let last = progress.last().and_then(|p| p.current.clone());
    assert_eq!(last, plan.steps.last().map(super::super::run::current_of));
}

fn cancelled_mid_job(workers: usize) -> (FakeFs, OperationPlan, JobOutcome, Harness) {
    let fs = many_files();
    let plan = copy_many(&fs);
    let mut harness = Harness {
        cancel_after_bytes: Some(BIG + 6_000),
        ..with_workers(workers)
    };
    let outcome = harness.run(&fs, &plan, &[]);
    (fs, plan, outcome, harness)
}

#[test]
fn cancel_mid_parallel_job_ends_cancelled() {
    for workers in WORKERS {
        assert_eq!(cancelled_mid_job(workers).2, JobOutcome::Cancelled);
    }
}

#[test]
fn cancel_mid_parallel_job_leaves_no_temporary_file() {
    for workers in WORKERS {
        assert_eq!(
            leftovers(&cancelled_mid_job(workers).0),
            Vec::<String>::new()
        );
    }
}

#[test]
fn cancel_mid_parallel_job_journals_only_files_in_place() {
    for workers in WORKERS {
        let (fs, plan, _, harness) = cancelled_mid_job(workers);
        let logged = harness.logged();
        assert!(logged.iter().all(|(_, o)| *o != StepOutcome::Failed));
        assert!(placed_exactly_the_done_copies(
            &fs,
            &plan,
            &harness.log.done()
        ));
    }
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    wake: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().expect("gate") = true;
        self.wake.notify_all();
    }

    fn wait(&self) {
        let open = self.open.lock().expect("gate");
        let _ = self.wake.wait_timeout_while(open, WAIT, |open| !*open);
    }
}

pub(super) struct Hooked<'a> {
    pub(super) fs: &'a FakeFs,
    pub(super) before_copy: &'a (dyn Fn(&str, &CancelToken) + Sync),
    pub(super) on_chunk: &'a (dyn Fn(&str, u64) + Sync),
}

pub(super) fn no_chunk_hook(_: &str, _: u64) {}

impl Provider for Hooked<'_> {
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

impl ProviderOps for Hooked<'_> {
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
        let path = source.path().to_text();
        (self.before_copy)(&path, cancel);
        let mut chunk = |bytes: u64| {
            (self.on_chunk)(&path, bytes);
            progress(bytes);
        };
        self.fs.copy_file(source, target, &mut chunk, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_file(location)
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

struct OpensOnFailure<'a> {
    log: Log,
    gate: &'a Gate,
}

impl JobLog for OpensOnFailure<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome == StepOutcome::Failed {
            self.gate.open();
        }
        self.log.step(seq, outcome)
    }
}

const FLAT: usize = 10;

struct Held {
    fs: FakeFs,
    outcome: JobOutcome,
    logged: Vec<(u32, StepOutcome)>,
    progress: Vec<Progress>,
}

fn held_until_a_failure(failing: &[(usize, Fault)]) -> Held {
    let fs = flat(FLAT);
    let plan = copy_flat(&fs, FLAT);
    let gate = Gate::default();
    let first = failing[0].0;
    let held: Vec<String> = (0..FLAT)
        .filter(|i| *i != first)
        .map(|i| format!("src/f{i}.txt"))
        .collect();
    let hold = |path: &str, _: &CancelToken| {
        if held.iter().any(|h| h == path) {
            gate.wait();
        }
    };
    let ops = Hooked {
        fs: &fs,
        before_copy: &hold,
        on_chunk: &no_chunk_hook,
    };
    for (index, fault) in failing {
        fs.fault(&format!("src/f{index}.txt"), FakeOp::Copy, 1, fault.clone());
    }
    let mut log = OpensOnFailure {
        log: Log::default(),
        gate: &gate,
    };
    let cancel = CancelToken::new();
    let mut progress = Vec::new();
    let mut ctx = JobContext {
        ops: &ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut |p: &Progress| progress.push(p.clone()),
    };
    let outcome = super::super::parallel::run(&plan, &[], &mut ctx, 4);
    Held {
        fs,
        outcome,
        logged: sorted(log.log.entries),
        progress,
    }
}

fn locked() -> Fault {
    Fault::Fail(FsErrorKind::Locked)
}

#[test]
fn the_first_parallel_failure_starts_no_new_copy() {
    let Held { fs, .. } = held_until_a_failure(&[(1, locked())]);
    let started: Vec<u32> = (0..FLAT)
        .map(|i| fs.calls(&format!("src/f{i}.txt"), FakeOp::Copy))
        .collect();
    assert_eq!(started, [1, 1, 1, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn the_first_parallel_failure_is_reported() {
    let Held { outcome, .. } = held_until_a_failure(&[(1, locked())]);
    assert_eq!(failure(&outcome), Some((1, FsErrorKind::Locked)));
}

#[test]
fn copies_in_flight_at_a_failure_finish_and_are_journalled() {
    let Held { fs, logged, .. } = held_until_a_failure(&[(1, locked())]);
    assert_eq!(
        (logged, tree_of(&fs, "dst"), leftovers(&fs)),
        (
            vec![
                (0, StepOutcome::Done),
                (1, StepOutcome::Failed),
                (2, StepOutcome::Done),
                (3, StepOutcome::Done),
            ],
            ["f0.txt", "f2.txt", "f3.txt"]
                .map(|name| entry(name, EntryKind::File, BIG))
                .to_vec(),
            Vec::new()
        )
    );
}

#[test]
fn a_parallel_failure_reports_the_lowest_failed_step_not_the_first_seen() {
    let Held {
        outcome, logged, ..
    } = held_until_a_failure(&[(2, locked()), (1, Fault::Fail(FsErrorKind::AccessDenied))]);
    assert_eq!(
        (failure(&outcome), logged),
        (
            Some((1, FsErrorKind::AccessDenied)),
            vec![
                (0, StepOutcome::Done),
                (1, StepOutcome::Failed),
                (2, StepOutcome::Failed),
                (3, StepOutcome::Done),
            ]
        )
    );
}

#[test]
fn progress_never_goes_back_when_a_copy_in_flight_fails() {
    let fault = Fault::FailAfterBytes {
        kind: FsErrorKind::DiskFull,
        bytes: 8_000,
    };
    let Held { progress, .. } = held_until_a_failure(&[(1, fault)]);
    let bytes: Vec<u64> = progress.iter().map(|p| p.bytes_done).collect();
    assert!(bytes.windows(2).all(|pair| pair[0] <= pair[1]), "{bytes:?}");
}

#[test]
fn a_panicking_copy_fails_its_step_and_leaves_no_temporary_file() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let panic_in_two = |path: &str, _: u64| assert_ne!(path, "src/two.txt", "copy panicked");
    let ops = Hooked {
        fs: &fs,
        before_copy: &|_: &str, _: &CancelToken| {},
        on_chunk: &panic_in_two,
    };
    let (outcome, _) = run_in(4, &ops, &plan);
    assert_eq!(
        (
            failure(&outcome),
            leftovers(&fs),
            exists(&fs, "dst/two.txt")
        ),
        (Some((1, FsErrorKind::Unknown)), Vec::new(), false)
    );
}

#[test]
fn a_journal_write_failure_fails_a_parallel_job_at_that_step() {
    let fs = three_files();
    let plan = copy_three(&fs);
    let mut harness = Harness {
        log: Log {
            fail_on: Some(1),
            ..Log::default()
        },
        ..with_workers(4)
    };
    assert_eq!(
        harness.run(&fs, &plan, &[]),
        JobOutcome::Failed {
            seq: 1,
            error: log_error()
        }
    );
}

#[test]
fn a_parallel_move_that_fails_while_copying_removes_no_source() {
    let fs = many_files();
    let plan = move_many(&fs);
    let before = tree_of(&fs, "src");
    fs.fault(
        "src/a/b/c/c1.txt",
        FakeOp::Copy,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let mut harness = with_workers(8);
    let outcome = harness.run(&fs, &plan, &[]);
    assert_eq!(
        (failure(&outcome).map(|(_, kind)| kind), tree_of(&fs, "src")),
        (Some(FsErrorKind::NetworkLost), before)
    );
}

fn crashed_in_parallel(workers: usize) -> (FakeFs, OperationPlan, Vec<u32>) {
    let fs = many_files();
    let plan = copy_many(&fs);
    let done = crashed_with(&fs, &plan, "src/a/b/b0.txt", workers);
    (fs, plan, done)
}

#[test]
fn a_parallel_crash_journals_only_files_in_place() {
    for workers in WORKERS {
        let (fs, plan, done) = crashed_in_parallel(workers);
        assert!(
            placed_exactly_the_done_copies(&fs, &plan, &done),
            "{workers}"
        );
    }
}

#[test]
fn a_parallel_job_resumed_after_a_crash_equals_a_clean_run() {
    let clean = ending(many_files, copy_many, 0).1;
    for workers in WORKERS {
        let (fs, plan, done) = crashed_in_parallel(workers);
        let _ = clean_up(&plan, &done, &fs);
        let mut harness = with_workers(workers);
        let outcome = harness.run(&fs, &plan, &done);
        let redone = sorted(harness.logged())
            .into_iter()
            .filter(|(seq, _)| done.contains(seq))
            .count();
        assert_eq!(
            (outcome, tree_of(&fs, ""), leftovers(&fs), redone),
            (JobOutcome::Done, clean.clone(), Vec::new(), 0),
            "{workers} workers"
        );
    }
}

#[test]
fn a_parallel_crash_rolls_back_to_the_starting_tree() {
    let before = tree_of(&many_files(), "");
    let (fs, plan, done) = crashed_in_parallel(8);
    let _ = clean_up(&plan, &done, &fs);
    let mut next_start = Harness::default();
    assert_eq!(next_start.roll_back(&fs, &plan, &done), JobOutcome::Done);
    assert_eq!((tree_of(&fs, ""), leftovers(&fs)), (before, Vec::new()));
}

fn kept_somewhere(fs: &FakeFs, files: &[(&str, &str, u64)]) -> bool {
    files.iter().all(|(source, target, bytes)| {
        [*source, *target]
            .iter()
            .any(|p| size(fs, p) == Some(*bytes))
    })
}

const TWINS: [(&str, &str, u64); 2] = [
    ("src/a/x.txt", "dst/x.txt", 5_000),
    ("src/b/x.txt", "dst/x.txt", 6_000),
];

fn twins() -> FakeFs {
    fixture(&TWINS.map(|(source, _, bytes)| (source, bytes)))
}

fn move_twins(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        ..request(fs, OpKind::Move, &["src/a/x.txt", "src/b/x.txt"], "dst")
    })
}

#[test]
fn a_parallel_move_of_two_files_with_one_target_loses_neither() {
    ends_like_standard(twins, move_twins);
    let fs = twins();
    let _ = run_in(8, &fs, &move_twins(&fs));
    assert!(kept_somewhere(&fs, &TWINS));
}

const NEIGHBOURS: [(&str, &str, u64); 2] = [
    ("src/x.ferrite-part", "dst/x.ferrite-part", 5_000),
    ("src/x", "dst/x", 6_000),
];

fn neighbours() -> FakeFs {
    fixture(&NEIGHBOURS.map(|(source, _, bytes)| (source, bytes)))
}

fn move_neighbours(fs: &FakeFs) -> OperationPlan {
    plan(&request(
        fs,
        OpKind::Move,
        &["src/x.ferrite-part", "src/x"],
        "dst",
    ))
}

#[test]
fn a_parallel_move_of_a_file_next_to_its_temporary_name_loses_neither() {
    ends_like_standard(neighbours, move_neighbours);
    let fs = neighbours();
    let _ = run_in(8, &fs, &move_neighbours(&fs));
    assert!(kept_somewhere(&fs, &NEIGHBOURS));
}

fn copy_step((source, target, size): (&str, &str, u64), fs: &FakeFs) -> Step {
    Step::CopyFile {
        source: fs.location(source),
        target: fs.location(target),
        size,
    }
}

fn hand_built(fs: &FakeFs, files: [(&str, &str, u64); 2]) -> OperationPlan {
    let sources = files.map(|(source, _, _)| source);
    let mut plan = plan(&request(fs, OpKind::Move, &sources, "dst"));
    plan.conflicts.clear();
    plan.steps = files.iter().map(|file| copy_step(*file, fs)).collect();
    plan.steps
        .extend(files.iter().map(|(source, _, _)| Step::RemoveFile {
            location: fs.location(source),
        }));
    plan
}

fn hand_built_ends_like_standard(build: fn() -> FakeFs, files: [(&str, &str, u64); 2]) {
    let standard = ending(build, |fs| hand_built(fs, files), 0);
    for _ in 0..20 {
        assert_eq!(ending(build, |fs| hand_built(fs, files), 8), standard);
        let fs = build();
        let _ = run_in(8, &fs, &hand_built(&fs, files));
        assert!(kept_somewhere(&fs, &files));
    }
}

#[test]
fn copies_that_share_a_target_run_in_plan_order() {
    hand_built_ends_like_standard(twins, TWINS);
}

#[test]
fn a_copy_whose_temporary_name_is_another_target_runs_in_plan_order() {
    let reversed = [NEIGHBOURS[1], NEIGHBOURS[0]];
    hand_built_ends_like_standard(neighbours, reversed);
}

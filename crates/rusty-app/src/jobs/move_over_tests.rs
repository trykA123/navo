use std::sync::Arc;

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{CancelToken, FakeFs, JobContext, JobOutcome, Provider, existing_at, run_job, scan};

use super::held_tests::{
    add_timed, fake, held_rows, holding, journal, old_report, source_report, target,
};
use super::{SharedJournal, log_factory, recover};

const SOURCE: &str = "home/docs/report.txt";

fn move_plan(fs: &FakeFs) -> OperationPlan {
    let cancel = CancelToken::new();
    let items = scan(fs, &[fs.location(SOURCE)], &cancel).unwrap();
    let folder = fs.location("dest/docs");
    let existing = existing_at(fs, &items, &folder, &cancel).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Move,
        items,
        target: Some(folder),
        existing,
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: Vec::new(),
    })
}

fn moved_over(fs: &FakeFs, journal: &SharedJournal) {
    let plan = move_plan(fs);
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
}

fn size_at(fs: &FakeFs, path: &str) -> Option<u64> {
    fs.metadata(&fs.location(path)).ok().map(|entry| entry.size)
}

#[test]
fn a_move_over_a_file_holds_the_old_file_while_the_job_runs() {
    let fs = fake();
    let journal = journal();
    moved_over(&fs, &journal);
    assert_eq!(
        (
            target(&fs).map(|e| e.size),
            size_at(&fs, SOURCE),
            holding(&fs).len(),
            held_rows(&journal)
        ),
        (Some(7), None, 1, 1)
    );
}

#[test]
fn a_move_over_a_file_cut_short_gets_both_files_back_at_start() {
    let fs = fake();
    let journal = journal();
    moved_over(&fs, &journal);
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert_eq!(
        (
            target(&fs),
            size_at(&fs, SOURCE),
            holding(&fs),
            held_rows(&journal)
        ),
        (Some(old_report()), Some(7), Vec::new(), 0)
    );
}

#[test]
fn a_move_over_a_file_changed_since_keeps_the_new_file_and_puts_the_old_beside() {
    let fs = fake();
    let journal = journal();
    moved_over(&fs, &journal);
    fs.remove("dest/docs/report.txt");
    add_timed(&fs, "dest/docs", "report.txt", 8);
    source_report(&fs, 7);
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert_eq!(
        (
            size_at(&fs, "dest/docs/report.txt"),
            size_at(&fs, "dest/docs/report (before copy).txt"),
            size_at(&fs, SOURCE),
        ),
        (Some(8), Some(2), Some(7))
    );
}

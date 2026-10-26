use std::sync::{Arc, Mutex};

use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;
use rusty_core::{FsError, FsErrorKind, OpKind, OperationPlan, Step, Totals};
use rusty_fs::{
    CancelToken, EntryBatch, FakeFs, FakeOp, Fault, HeldFile, JobContext, JobLog, JobOutcome,
    Provider, ProviderOps, RenameOutcome, StepOutcome, run_job, run_rename,
};
use rusty_store::{Db, JobJournal};
use rusty_ui::browser::OpLog;

use super::held_tests::journal;
use super::{SharedJournal, lock, log_factory, recover, still_pending, undo_log};

fn folder(names: &[&str]) -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("d");
    for (index, name) in (1_u64..).zip(names) {
        fs.add_file(&format!("d/{name}"), index * 10);
    }
    fs
}

fn batch(fs: &FakeFs, names: &[&str], pairs: &[(&str, &str)]) -> OperationPlan {
    let rows: Vec<Row> = pairs
        .iter()
        .map(|(old, new)| Row {
            parent: fs.location("d"),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            excluded: false,
        })
        .collect();
    let existing = names
        .iter()
        .map(|name| ExistingName {
            parent: fs.location("d"),
            name: (*name).to_owned(),
        })
        .collect();
    plan_batch(&rows, &Existing::new(existing), Fold::Exact, 7)
        .plan
        .expect("a plan")
}

fn rename_of(plan: &OperationPlan, seq: usize) -> (String, String) {
    match &plan.steps[seq] {
        Step::Rename { source, target } => (source.path().to_text(), target.path().to_text()),
        other => panic!("unexpected step {other:?}"),
    }
}

fn sizes(fs: &FakeFs, names: &[&str]) -> Vec<Option<u64>> {
    names
        .iter()
        .map(|name| {
            fs.metadata(&fs.location(&format!("d/{name}")))
                .ok()
                .map(|entry| entry.size)
        })
        .collect()
}

fn entries(fs: &FakeFs) -> usize {
    let mut count = 0;
    let mut sink = |batch: EntryBatch| {
        count += batch.entries.len();
        std::ops::ControlFlow::Continue(())
    };
    fs.list(&fs.location("d"), &mut sink, &CancelToken::new())
        .unwrap();
    count
}

fn rename_with(fs: &FakeFs, plan: &OperationPlan, log: &mut dyn OpLog) -> RenameOutcome {
    run_rename(
        plan,
        &mut JobContext {
            ops: fs,
            log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    )
}

fn pending(journal: &SharedJournal) -> usize {
    lock(journal).restore_pending().unwrap().len()
}

const SWAP: [&str; 3] = ["a", "b", "x"];

mod identity;

fn steps(fs: &FakeFs, moves: &[(&str, &str)]) -> OperationPlan {
    let at = |name: &str| fs.location(&format!("d/{name}"));
    OperationPlan {
        kind: OpKind::Rename,
        steps: moves
            .iter()
            .map(|(from, to)| Step::Rename {
                source: at(from),
                target: at(to),
            })
            .collect(),
        totals: Totals {
            files: 3,
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

#[test]
fn a_put_back_batch_cut_short_before_finish_keeps_the_original_names() {
    let fs = folder(&SWAP);
    let temp = ".ferrite-rename-00000007-0";
    let plan = steps(&fs, &[("a", temp), ("b", "a"), (temp, "b"), ("x", "y")]);
    fs.fault("d/x", FakeOp::Rename, 1, Fault::Fail(FsErrorKind::Locked));
    let journal = journal();
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let outcome = rename_with(&fs, &plan, log.as_mut());
    assert!(
        matches!(outcome, RenameOutcome::PutBack { seq: 3, .. }),
        "{outcome:?}"
    );
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &["a", "b", "x", "y"]), entries(&fs)),
        (vec![Some(10), Some(20), Some(30), None], 3)
    );
}

mod lost_write;
use lost_write::crash_before_the_journal_write;

#[test]
fn a_crash_after_a_rename_before_its_journal_write_is_put_back_at_start() {
    let fs = folder(&["a", "b"]);
    let plan = batch(&fs, &["a", "b"], &[("a", "b"), ("b", "a")]);
    let journal = journal();
    crash_before_the_journal_write(&fs, &plan, &journal);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &["a", "b"]), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20)], 2, 0)
    );
}

#[test]
fn a_rename_recovery_that_fails_stays_pending_and_is_retried() {
    let fs = folder(&["a", "b"]);
    let plan = batch(&fs, &["a", "b"], &[("a", "b"), ("b", "a")]);
    let journal = journal();
    crash_before_the_journal_write(&fs, &plan, &journal);
    let renamed = rename_of(&plan, 1).1;
    let calls = fs.calls(&renamed, FakeOp::Rename);
    for next in calls + 1..=calls + 2 {
        fs.fault(
            &renamed,
            FakeOp::Rename,
            next,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
    }
    recover(&journal, &fs).unwrap();
    assert_eq!(pending(&journal), 1);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &["a", "b"]), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20)], 2, 0)
    );
}

#[test]
fn a_failed_put_back_finishes_pending_and_recovery_completes_it() {
    let names = ["a", "b", "c"];
    let fs = folder(&names);
    let plan = batch(&fs, &names, &[("a", "x"), ("b", "y"), ("c", "z")]);
    fs.fault(
        &rename_of(&plan, 2).0,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    fs.fault(
        &rename_of(&plan, 1).1,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let journal = journal();
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let outcome = rename_with(&fs, &plan, log.as_mut());
    let RenameOutcome::Stuck { error, .. } = outcome else {
        panic!("expected Stuck, got {outcome:?}");
    };
    log.finish(
        &JobOutcome::Failed { seq: 2, error },
        &fs as &dyn ProviderOps,
    )
    .unwrap();
    assert_eq!(pending(&journal), 1);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &names), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20), Some(30)], 3, 0)
    );
}

fn renamed(fs: &FakeFs, journal: &SharedJournal, pairs: &[(&str, &str)]) -> OperationPlan {
    let names: Vec<String> = ["a", "b", "c", "x", "y", "z"]
        .iter()
        .filter(|name| fs.metadata(&fs.location(&format!("d/{name}"))).is_ok())
        .map(|name| (*name).to_owned())
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let plan = batch(fs, &names, pairs);
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    assert_eq!(rename_with(fs, &plan, log.as_mut()), RenameOutcome::Done);
    log.finish(&JobOutcome::Done, fs as &dyn ProviderOps)
        .unwrap();
    plan
}

fn newest(journal: &SharedJournal) -> Option<rusty_store::JobId> {
    lock(journal)
        .last_undoable("rename")
        .unwrap()
        .map(|job| job.id)
}

#[test]
fn an_undo_log_finished_done_marks_the_batch_undone_and_offers_the_older_one() {
    let fs = folder(&["a", "b", "c"]);
    let journal = journal();
    renamed(&fs, &journal, &[("c", "z")]);
    let older = newest(&journal).unwrap();
    renamed(&fs, &journal, &[("a", "x"), ("b", "y")]);
    let job = newest(&journal).unwrap();
    let mut log = undo_log(&journal, job).unwrap();
    log.undone(0, StepOutcome::Done).unwrap();
    log.undone(1, StepOutcome::Skipped).unwrap();
    log.finish(&JobOutcome::Done, &fs as &dyn ProviderOps)
        .unwrap();
    assert_eq!(newest(&journal), Some(older));
    assert_eq!(pending(&journal), 0);
}

#[test]
fn an_undo_log_with_a_failed_step_stays_pending() {
    let fs = folder(&["a", "b"]);
    let journal = journal();
    renamed(&fs, &journal, &[("a", "x"), ("b", "y")]);
    let job = newest(&journal).unwrap();
    let mut log = undo_log(&journal, job).unwrap();
    log.undone(1, StepOutcome::Failed).unwrap();
    let error = FsError::new(FsErrorKind::AccessDenied);
    log.finish(
        &JobOutcome::Failed { seq: 1, error },
        &fs as &dyn ProviderOps,
    )
    .unwrap();
    assert_eq!((newest(&journal), pending(&journal)), (Some(job), 1));
}

#[test]
fn a_crash_during_an_undo_is_finished_at_start() {
    let fs = folder(&["a", "b", "c"]);
    let journal = journal();
    let plan = renamed(&fs, &journal, &[("a", "x"), ("b", "y"), ("c", "z")]);
    let job = newest(&journal).unwrap();
    let mut log = undo_log(&journal, job).unwrap();
    let (source, target) = rename_of(&plan, 2);
    fs.rename_entry(&target, &source);
    log.undone(2, StepOutcome::Done).unwrap();
    drop(log);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            sizes(&fs, &["a", "b", "c"]),
            entries(&fs),
            pending(&journal)
        ),
        (vec![Some(10), Some(20), Some(30)], 3, 0)
    );
    assert_eq!(newest(&journal), None);
}

#[test]
fn a_recovery_stops_at_a_failed_step_and_the_retry_finishes_the_swap() {
    let fs = folder(&["a", "b"]);
    let journal = journal();
    let plan = renamed(&fs, &journal, &[("a", "b"), ("b", "a")]);
    assert_eq!(rename_of(&plan, 2).1, "d/b");
    let job = newest(&journal).unwrap();
    drop(undo_log(&journal, job).unwrap());
    let next = fs.calls("d/b", FakeOp::Rename) + 1;
    fs.fault(
        "d/b",
        FakeOp::Rename,
        next,
        Fault::Fail(FsErrorKind::Locked),
    );
    recover(&journal, &fs).unwrap();
    assert_eq!(pending(&journal), 1);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &["a", "b"]), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20)], 2, 0)
    );
}

fn journal_losing_put_back_records_while_running() -> SharedJournal {
    let db = Db::open_in_memory().unwrap();
    db.connection()
        .execute_batch(
            "CREATE TRIGGER lose_undone BEFORE UPDATE OF detail ON job_step
             WHEN NEW.detail = CAST('undone' AS BLOB)
              AND (SELECT state FROM job WHERE id = NEW.job_id) = 'running'
             BEGIN SELECT RAISE(ABORT, 'lost'); END;",
        )
        .unwrap();
    Arc::new(Mutex::new(JobJournal::new(db).unwrap()))
}

fn swap_stuck_on_a_lost_put_back_record(fs: &FakeFs, journal: &SharedJournal) -> OperationPlan {
    let plan = batch(fs, &["a", "b"], &[("a", "b"), ("b", "a")]);
    fs.fault(
        &rename_of(&plan, 2).0,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    let outcome = rename_with(fs, &plan, log.as_mut());
    let RenameOutcome::Stuck { error, .. } = outcome else {
        panic!("expected Stuck, got {outcome:?}");
    };
    log.finish(
        &JobOutcome::Failed { seq: 2, error },
        fs as &dyn ProviderOps,
    )
    .unwrap();
    plan
}

#[test]
fn a_put_back_stopped_by_a_lost_journal_write_stays_pending_and_recovery_completes_it() {
    let fs = folder(&["a", "b"]);
    let journal = journal_losing_put_back_records_while_running();
    swap_stuck_on_a_lost_put_back_record(&fs, &journal);
    assert_eq!(pending(&journal), 1);
    assert!(newest(&journal).is_some());
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &["a", "b"]), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20)], 2, 0)
    );
}

#[test]
fn a_put_back_that_keeps_failing_at_start_lists_the_file_under_its_temporary_name() {
    let fs = folder(&["a", "b"]);
    let journal = journal_losing_put_back_records_while_running();
    let plan = swap_stuck_on_a_lost_put_back_record(&fs, &journal);
    let temp = rename_of(&plan, 0).1;
    let next = fs.calls(&temp, FakeOp::Rename) + 1;
    fs.fault(
        &temp,
        FakeOp::Rename,
        next,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    recover(&journal, &fs).unwrap();
    let left = still_pending(&journal).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].left, [fs.location(&temp)]);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (
            sizes(&fs, &["a", "b"]),
            still_pending(&journal).unwrap().len()
        ),
        (vec![Some(10), Some(20)], 0)
    );
}

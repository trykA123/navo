use std::sync::Arc;

use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;
use rusty_core::{FsError, FsErrorKind, OpKind, OperationPlan, Step, Totals};
use rusty_fs::{
    CancelToken, EntryBatch, FakeFs, FakeOp, Fault, HeldFile, JobContext, JobLog, JobOutcome,
    Provider, ProviderOps, RenameOutcome, StepOutcome, run_job, run_rename,
};
use rusty_ui::browser::OpLog;

use super::held_tests::journal;
use super::{SharedJournal, lock, log_factory, recover};

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

struct LostWrite {
    inner: Box<dyn OpLog>,
    at: u32,
}

impl JobLog for LostWrite {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if seq == self.at {
            return Err(FsError::new(FsErrorKind::Unknown));
        }
        self.inner.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.inner.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.inner.journal_held(held)
    }
}

fn crash_before_the_journal_write(fs: &FakeFs, plan: &OperationPlan, journal: &SharedJournal) {
    let mut log = LostWrite {
        inner: log_factory(Arc::clone(journal))(plan).unwrap(),
        at: 1,
    };
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert!(matches!(outcome, JobOutcome::Failed { seq: 1, .. }));
}

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
    );
    assert_eq!(pending(&journal), 1);
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (sizes(&fs, &names), entries(&fs), pending(&journal)),
        (vec![Some(10), Some(20), Some(30)], 3, 0)
    );
}

use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;

use super::*;
use crate::RenameOutcome;

const SEED: u32 = 7;

fn rows(fs: &FakeFs, pairs: &[(&str, &str)]) -> Vec<Row> {
    pairs
        .iter()
        .map(|(old, new)| Row {
            parent: fs.location("d"),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            excluded: false,
        })
        .collect()
}

fn batch(fs: &FakeFs, pairs: &[(&str, &str)], fold: Fold) -> OperationPlan {
    let names = children(fs, "d")
        .iter()
        .map(|entry| ExistingName {
            parent: fs.location("d"),
            name: entry.name.to_text(),
        })
        .collect();
    plan_batch(&rows(fs, pairs), &Existing::new(names), fold, SEED)
        .plan
        .expect("a plan")
}

fn files(names: &[&str]) -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("d");
    for (index, name) in (1_u64..).zip(names) {
        fs.add_file(&format!("d/{name}"), index * 10);
    }
    fs
}

fn names(fs: &FakeFs) -> Vec<String> {
    let mut names: Vec<String> = children(fs, "d")
        .iter()
        .map(|entry| entry.name.to_text())
        .collect();
    names.sort();
    names
}

fn run_with(
    fs: &FakeFs,
    plan: &OperationPlan,
    cancel_after_files: Option<u64>,
) -> (RenameOutcome, Vec<Progress>, Log) {
    run_token(fs, plan, &CancelToken::new(), cancel_after_files)
}

fn run_token(
    fs: &FakeFs,
    plan: &OperationPlan,
    cancel: &CancelToken,
    cancel_after_files: Option<u64>,
) -> (RenameOutcome, Vec<Progress>, Log) {
    let mut log = Log::default();
    let mut seen = Vec::new();
    let mut on_progress = |progress: &Progress| {
        seen.push(progress.clone());
        if cancel_after_files.is_some_and(|files| progress.files_done >= files) {
            cancel.cancel();
        }
    };
    let outcome = {
        let mut ctx = JobContext {
            ops: fs,
            log: &mut log,
            cancel,
            progress: &mut on_progress,
        };
        crate::run_rename(plan, &mut ctx)
    };
    (outcome, seen, log)
}

fn run_batch(fs: &FakeFs, plan: &OperationPlan) -> RenameOutcome {
    run_with(fs, plan, None).0
}

fn sources(plan: &OperationPlan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|step| match step {
            Step::Rename { source, .. } => source.path().to_text(),
            other => panic!("unexpected step {other:?}"),
        })
        .collect()
}

fn targets(plan: &OperationPlan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|step| match step {
            Step::Rename { target, .. } => target.path().to_text(),
            other => panic!("unexpected step {other:?}"),
        })
        .collect()
}

const SIMPLE: [(&str, &str); 3] = [("a", "x"), ("b", "y"), ("c", "z")];

fn simple() -> (FakeFs, OperationPlan) {
    let fs = files(&["a", "b", "c"]);
    let plan = batch(&fs, &SIMPLE, Fold::Exact);
    (fs, plan)
}

#[test]
fn a_batch_renames_every_item() {
    let (fs, plan) = simple();
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(names(&fs), ["x", "y", "z"]);
    assert_eq!(
        [size(&fs, "d/x"), size(&fs, "d/y"), size(&fs, "d/z")],
        [Some(10), Some(20), Some(30)]
    );
}

#[test]
fn progress_counts_each_item_once_and_never_a_temporary_step() {
    let fs = files(&["a", "b"]);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")], Fold::Exact);
    assert_eq!(plan.steps.len(), 3);
    let (outcome, seen, _) = run_with(&fs, &plan, None);
    assert_eq!(outcome, RenameOutcome::Done);
    let done: Vec<u64> = seen.iter().map(|progress| progress.files_done).collect();
    assert_eq!((done, seen[0].files_total), (vec![0, 1, 2], 2));
}

#[test]
fn a_swap_exchanges_the_two_files() {
    let fs = files(&["a", "b"]);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")], Fold::Exact);
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(
        (size(&fs, "d/a"), size(&fs, "d/b"), names(&fs)),
        (Some(20), Some(10), vec!["a".to_owned(), "b".to_owned()])
    );
}

#[test]
fn a_fault_at_any_step_puts_everything_back() {
    for step in 0..SIMPLE.len() {
        let (fs, plan) = simple();
        let before = tree_of(&fs, "d");
        fs.fault(
            &sources(&plan)[step],
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::Locked),
        );
        let outcome = run_batch(&fs, &plan);
        let seq = u32::try_from(step).unwrap();
        assert_eq!(
            matches!(&outcome, RenameOutcome::PutBack { seq: at, error }
                if *at == seq && error.kind == FsErrorKind::Locked),
            true,
            "step {step}: {outcome:?}"
        );
        assert_eq!(tree_of(&fs, "d"), before, "step {step}");
    }
}

#[test]
fn a_fault_in_a_swap_puts_the_swap_back() {
    for step in 0..3 {
        let fs = files(&["a", "b"]);
        let plan = batch(&fs, &[("a", "b"), ("b", "a")], Fold::Exact);
        let before = tree_of(&fs, "d");
        fs.fault(
            &sources(&plan)[step],
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
        let outcome = run_batch(&fs, &plan);
        assert!(
            matches!(outcome, RenameOutcome::PutBack { .. }),
            "{outcome:?}"
        );
        assert_eq!(tree_of(&fs, "d"), before, "step {step}");
    }
}

#[test]
fn a_cancel_after_some_renames_puts_everything_back() {
    for after in 1..=2 {
        let (fs, plan) = simple();
        let before = tree_of(&fs, "d");
        let (outcome, _, _) = run_with(&fs, &plan, Some(after));
        assert_eq!(outcome, RenameOutcome::Cancelled, "after {after}");
        assert_eq!(tree_of(&fs, "d"), before, "after {after}");
    }
}

#[test]
fn a_cancel_before_the_start_changes_nothing() {
    let (fs, plan) = simple();
    let before = tree_of(&fs, "d");
    let cancel = CancelToken::new();
    cancel.cancel();
    let (outcome, _, log) = run_token(&fs, &plan, &cancel, None);
    assert_eq!(outcome, RenameOutcome::Cancelled);
    assert_eq!((tree_of(&fs, "d"), log.entries), (before, Vec::new()));
}

#[test]
fn putting_back_logs_each_undone_step_in_reverse() {
    let (fs, plan) = simple();
    fs.fault(
        &sources(&plan)[2],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let (_, _, log) = run_with(&fs, &plan, None);
    let done = StepOutcome::Done;
    assert_eq!(
        log.entries,
        [
            (0, done),
            (1, done),
            (2, StepOutcome::Failed),
            (1, done),
            (0, done)
        ]
    );
}

#[test]
fn a_failure_while_putting_back_lists_the_files_left() {
    let (fs, plan) = simple();
    fs.fault(
        &sources(&plan)[2],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    fs.fault(
        &targets(&plan)[1],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let outcome = run_batch(&fs, &plan);
    let RenameOutcome::Stuck { error, left } = outcome else {
        panic!("expected Stuck, got {outcome:?}");
    };
    assert_eq!(error.kind, FsErrorKind::AccessDenied);
    assert_eq!(left, [fs.location("d/y")]);
    assert_eq!(names(&fs), ["a", "c", "y"]);
}

#[test]
fn a_failed_put_back_of_a_temporary_name_lists_the_temporary_file() {
    let fs = files(&["a", "b"]);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")], Fold::Exact);
    fs.fault(
        &sources(&plan)[2],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    fs.fault(
        &targets(&plan)[0],
        FakeOp::Rename,
        2,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let RenameOutcome::Stuck { left, .. } = run_batch(&fs, &plan) else {
        panic!("expected Stuck");
    };
    assert_eq!(left.len(), 1);
    assert!(left[0].path().to_text().contains(".ferrite-rename-"));
}

#[test]
fn a_collision_never_overwrites_and_puts_everything_back() {
    let (fs, plan) = simple();
    fs.fault(&sources(&plan)[1], FakeOp::Rename, 1, Fault::Collide);
    let outcome = run_batch(&fs, &plan);
    assert!(
        matches!(outcome, RenameOutcome::PutBack { seq: 1, .. }),
        "{outcome:?}"
    );
    assert_eq!(
        [
            size(&fs, "d/a"),
            size(&fs, "d/b"),
            size(&fs, "d/c"),
            size(&fs, "d/y")
        ],
        [Some(10), Some(20), Some(30), Some(1)]
    );
    assert!(!exists(&fs, "d/x"));
}

fn insensitive(names: &[&str]) -> FakeFs {
    let fs = FakeFs::new().case_insensitive();
    fs.add_folder("d");
    for (index, name) in (1_u64..).zip(names) {
        fs.add_file(&format!("d/{name}"), index * 10);
    }
    fs
}

#[test]
fn a_case_only_rename_runs_through_a_temporary_name() {
    let fs = insensitive(&["a.txt"]);
    let plan = batch(&fs, &[("a.txt", "A.txt")], Fold::Insensitive);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(names(&fs), ["A.txt"]);
    assert_eq!(size(&fs, "d/A.txt"), Some(10));
}

#[test]
fn a_failed_case_only_rename_keeps_the_old_spelling() {
    let fs = insensitive(&["a.txt"]);
    let plan = batch(&fs, &[("a.txt", "A.txt")], Fold::Insensitive);
    fs.fault(
        &sources(&plan)[1],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let outcome = run_batch(&fs, &plan);
    assert!(
        matches!(outcome, RenameOutcome::PutBack { seq: 1, .. }),
        "{outcome:?}"
    );
    assert_eq!(names(&fs), ["a.txt"]);
}

#[test]
fn a_case_only_cancel_keeps_the_old_spelling() {
    let fs = insensitive(&["a.txt", "b.txt"]);
    let plan = batch(
        &fs,
        &[("a.txt", "A.txt"), ("b.txt", "B.txt")],
        Fold::Insensitive,
    );
    let (outcome, _, _) = run_with(&fs, &plan, Some(1));
    assert_eq!(outcome, RenameOutcome::Cancelled);
    assert_eq!(names(&fs), ["a.txt", "b.txt"]);
}

#[test]
fn a_swap_works_on_the_case_insensitive_fake() {
    let fs = insensitive(&["a.txt", "b.txt"]);
    let plan = batch(
        &fs,
        &[("a.txt", "b.txt"), ("b.txt", "a.txt")],
        Fold::Insensitive,
    );
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(
        (size(&fs, "d/a.txt"), size(&fs, "d/b.txt")),
        (Some(20), Some(10))
    );
}

#[test]
fn a_swap_with_a_case_change_works_on_the_case_insensitive_fake() {
    let fs = insensitive(&["a.txt", "b.txt"]);
    let plan = batch(
        &fs,
        &[("a.txt", "B.txt"), ("b.txt", "A.txt")],
        Fold::Insensitive,
    );
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(names(&fs), ["A.txt", "B.txt"]);
    assert_eq!(
        (size(&fs, "d/A.txt"), size(&fs, "d/B.txt")),
        (Some(20), Some(10))
    );
}

#[test]
fn a_finished_batch_leaves_no_temporary_name() {
    let fs = files(&["a", "b", "c"]);
    let plan = batch(&fs, &[("a", "b"), ("b", "c"), ("c", "a")], Fold::Exact);
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert!(
        names(&fs)
            .iter()
            .all(|name| !name.starts_with(".ferrite-rename"))
    );
    assert_eq!(
        [size(&fs, "d/a"), size(&fs, "d/b"), size(&fs, "d/c")],
        [Some(30), Some(10), Some(20)]
    );
}

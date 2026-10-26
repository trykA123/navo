use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;

use super::*;
use crate::RenameOutcome;

const SEED: u32 = 7;

#[test]
fn an_unproved_volume_renames_without_a_proved_identity() {
    let (fs, plan) = simple();
    fs.set_volume_serial("d", None);
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert_eq!(names(&fs), ["x", "y", "z"]);
}

#[test]
fn an_unproved_rollback_puts_back_items_whose_fingerprint_still_matches() {
    let (fs, plan) = simple();
    fs.set_volume_serial("d", None);
    fs.fault(
        &sources(&plan)[1],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let outcome = run_batch(&fs, &plan);
    assert!(matches!(outcome, RenameOutcome::PutBack { seq: 1, .. }));
    assert_eq!(names(&fs), ["a", "b", "c"]);
}

#[test]
fn an_unproved_rollback_leaves_a_changed_target_where_it_is() {
    let (fs, plan) = simple();
    fs.set_volume_serial("d", None);
    fs.fault(
        &sources(&plan)[1],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let target = fs.location(&targets(&plan)[0]);
    let mut changed = false;
    let outcome = crate::run_rename(
        &plan,
        &mut JobContext {
            ops: &fs,
            log: &mut Log::default(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {
                if !changed && fs.metadata(&target).is_ok() {
                    changed = true;
                    let mut entry = fs.metadata(&target).unwrap();
                    entry.modified_ms = Some(2_000);
                    fs.remove(&target.path().to_text());
                    fs.add_entry("d", entry);
                }
            },
        },
    );
    assert!(
        matches!(outcome, RenameOutcome::Stuck { error, .. } if error.kind == FsErrorKind::IdentityUnavailable)
    );
    assert_eq!(fs.metadata(&target).unwrap().modified_ms, Some(2_000));
}

const HOUR_MS: i64 = 3_600_000;

fn stamp(fs: &FakeFs, path: &str, size: u64, modified_ms: i64) {
    let location = fs.location(path);
    let mut entry = fs.metadata(&location).unwrap();
    entry.size = size;
    entry.modified_ms = Some(modified_ms);
    fs.remove(path);
    fs.add_entry(
        path.rsplit_once('/').map_or("", |(folder, _)| folder),
        entry,
    );
}

fn fingerprint_matches(fs: &FakeFs, path: &str, expected: Option<FileIdentity>) -> bool {
    fs.matches_identity(&fs.location(path), expected).unwrap()
}

#[test]
fn a_fingerprint_survives_a_daylight_saving_shift_of_exactly_one_hour() {
    let fs = files(&["a"]);
    stamp(&fs, "d/a", 10, 5 * HOUR_MS);
    let saved = Some(FileIdentity::fingerprint(
        &fs.metadata(&fs.location("d/a")).unwrap(),
    ));
    for shifted in [6 * HOUR_MS, 4 * HOUR_MS] {
        stamp(&fs, "d/a", 10, shifted);
        assert!(fingerprint_matches(&fs, "d/a", saved), "{shifted}");
    }
}

#[test]
fn a_fingerprint_refuses_other_time_shifts_and_any_size_change() {
    let fs = files(&["a"]);
    stamp(&fs, "d/a", 10, 5 * HOUR_MS);
    let saved = Some(FileIdentity::fingerprint(
        &fs.metadata(&fs.location("d/a")).unwrap(),
    ));
    for (size, modified_ms) in [
        (10, 5 * HOUR_MS + HOUR_MS / 2),
        (10, 5 * HOUR_MS + 2 * HOUR_MS),
        (10, 5 * HOUR_MS + HOUR_MS + 1),
        (11, 6 * HOUR_MS),
        (11, 5 * HOUR_MS),
    ] {
        stamp(&fs, "d/a", size, modified_ms);
        assert!(
            !fingerprint_matches(&fs, "d/a", saved),
            "{size} {modified_ms}"
        );
    }
}

fn inode_of(fs: &FakeFs, path: &str) -> Option<[u8; 16]> {
    let identity = fs.file_identity(&fs.location(path)).ok().flatten();
    identity.map(|identity| identity.file)
}

#[test]
fn identical_fingerprints_on_an_unproved_volume_roll_back_swaps_and_cycles() {
    let cases: [&[(&str, &str)]; 2] = [
        &[("a", "b"), ("b", "a")],
        &[("a", "b"), ("b", "c"), ("c", "a")],
    ];
    for pairs in cases {
        let fs = files(&["a", "b", "c"]);
        for name in ["a", "b", "c"] {
            stamp(&fs, &format!("d/{name}"), 10, 5 * HOUR_MS);
        }
        let before: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|name| inode_of(&fs, &format!("d/{name}")))
            .collect();
        fs.set_volume_serial("d", None);
        let plan = batch(&fs, pairs, Fold::Exact);
        fs.fault(
            &sources(&plan)[1],
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::Locked),
        );
        let outcome = run_batch(&fs, &plan);
        assert!(
            matches!(outcome, RenameOutcome::PutBack { .. }),
            "{pairs:?}"
        );
        assert_eq!(names(&fs), ["a", "b", "c"], "{pairs:?}");
        fs.set_volume_serial("d", Some(1));
        let after: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|name| inode_of(&fs, &format!("d/{name}")))
            .collect();
        assert_eq!(before, after, "{pairs:?}");
    }
}

#[test]
fn an_identity_read_error_refuses_the_step_and_puts_earlier_renames_back() {
    let (fs, plan) = simple();
    fs.fault(
        "d/b",
        FakeOp::Identity,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let outcome = run_batch(&fs, &plan);
    assert!(
        matches!(outcome, RenameOutcome::PutBack { seq: 1, error } if error.kind == FsErrorKind::AccessDenied)
    );
    assert_eq!(names(&fs), ["a", "b", "c"]);
    assert_eq!(fs.calls("d/b", FakeOp::Rename), 0);
}

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
        assert!(
            matches!(&outcome, RenameOutcome::PutBack { seq: at, error }
                if *at == seq && error.kind == FsErrorKind::Locked),
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
        log.undone,
        [(2, StepOutcome::Skipped), (1, done), (0, done)]
    );
}

#[test]
fn a_rename_whose_journal_write_fails_is_put_back() {
    let (fs, plan) = simple();
    let before = tree_of(&fs, "d");
    let mut log = Log {
        fail_on: Some(1),
        ..Log::default()
    };
    let outcome = {
        let mut ctx = JobContext {
            ops: &fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        };
        crate::run_rename(&plan, &mut ctx)
    };
    assert!(
        matches!(outcome, RenameOutcome::PutBack { seq: 1, .. }),
        "{outcome:?}"
    );
    assert_eq!(tree_of(&fs, "d"), before);
}

#[test]
fn a_failure_while_putting_back_stops_there_and_lists_the_files_left() {
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
    let RenameOutcome::Stuck {
        error,
        left,
        applied,
    } = outcome
    else {
        panic!("expected Stuck, got {outcome:?}");
    };
    assert_eq!(error.kind, FsErrorKind::AccessDenied);
    assert_eq!(left, [fs.location("d/x"), fs.location("d/y")]);
    assert_eq!(applied, [0, 1]);
    assert_eq!(names(&fs), ["c", "x", "y"]);
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
    assert!(
        left[0]
            .path()
            .to_text()
            .contains(rusty_core::brand::RENAME_PREFIX)
    );
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
fn both_temporary_prefixes_are_recognised() {
    let fs = files(&[".ferrite-rename-1", ".navo-rename-1", "plain"]);
    let temporary: Vec<bool> = names(&fs)
        .iter()
        .map(|name| crate::is_rename_temporary(&fs.location(&format!("d/{name}"))))
        .collect();
    assert_eq!(temporary, [true, true, false]);
}

#[test]
fn a_finished_batch_leaves_no_temporary_name() {
    let fs = files(&["a", "b", "c"]);
    let plan = batch(&fs, &[("a", "b"), ("b", "c"), ("c", "a")], Fold::Exact);
    assert_eq!(run_batch(&fs, &plan), RenameOutcome::Done);
    assert!(
        names(&fs)
            .iter()
            .all(|name| !rusty_core::brand::is_rename_temp(name))
    );
    assert_eq!(
        [size(&fs, "d/a"), size(&fs, "d/b"), size(&fs, "d/c")],
        [Some(30), Some(10), Some(20)]
    );
}

#[test]
fn a_rename_step_counts_as_an_item_only_in_a_rename_job() {
    let fs = FakeFs::new();
    let step = Step::Rename {
        source: fs.location("d/a"),
        target: fs.location("e/a"),
    };
    assert_eq!(crate::job::run::counted(OpKind::Rename, &step), (0, 1));
    assert_eq!(crate::job::run::counted(OpKind::Move, &step), (0, 0));
}

#[test]
fn opus_uncertain_rename_that_remains_applied_is_included_in_the_audit() {
    let (fs, plan) = simple();
    fs.fault(
        &sources(&plan)[0],
        FakeOp::Rename,
        1,
        Fault::RenameThenFail(FsErrorKind::Timeout),
    );
    fs.fault(
        &targets(&plan)[0],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let outcome = run_batch(&fs, &plan);
    let RenameOutcome::Stuck { applied, .. } = outcome else {
        panic!("expected Stuck");
    };
    assert_eq!(applied, [0]);
    assert!(fs.metadata(&fs.location(&targets(&plan)[0])).is_ok());
}

#[test]
fn opus_rollback_keeps_an_unconfirmed_target_pending_instead_of_claiming_put_back() {
    let (fs, plan) = simple();
    fs.fault(
        &sources(&plan)[1],
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let target = fs.location(&targets(&plan)[0]);
    let mut replaced = false;
    let outcome = crate::run_rename(
        &plan,
        &mut JobContext {
            ops: &fs,
            log: &mut Log::default(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {
                if !replaced && fs.metadata(&target).is_ok() {
                    replaced = true;
                    fs.remove_file(&target).unwrap();
                    fs.add_file(&target.path().to_text(), 99);
                }
            },
        },
    );
    assert!(
        matches!(outcome, RenameOutcome::Stuck { error, .. } if error.kind == FsErrorKind::IdentityUnavailable)
    );
    assert_eq!(fs.metadata(&target).unwrap().size, 99);
}

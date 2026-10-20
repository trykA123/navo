use super::kept_aside::{Watching, entry_at, holding, keeping, old_entry, over_old};
use super::*;

const OLD: u64 = 3;

pub(super) fn moved_entry() -> FsEntry {
    FsEntry {
        size: BIG,
        modified_ms: Some(2_000),
        ..old_entry("a.txt")
    }
}

pub(super) fn move_plan(fs: &FakeFs, same_volume: bool) -> OperationPlan {
    plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        same_volume,
        ..request(fs, OpKind::Move, &["src/a.txt"], "dst")
    })
}

pub(super) fn moving_over(same_volume: bool) -> (FakeFs, OperationPlan) {
    let (fs, _) = over_old(&["a.txt"]);
    let plan = move_plan(&fs, same_volume);
    (fs, plan)
}

fn moved(workers: usize) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = moving_over(true);
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undo_with(
    fs: &FakeFs,
    plan: &OperationPlan,
    held: Vec<HeldFile>,
    unreadable: bool,
) -> (JobOutcome, Log) {
    let mut harness = Harness::default();
    let done = all_seqs(plan);
    let job = Journaled {
        plan,
        done: &done,
        held: &held,
        placed: &[],
        held_unreadable: unreadable,
        unsure: None,
    };
    let outcome = harness.with(fs, |ctx| roll_back(&job, ctx));
    (outcome, harness.log)
}

fn undone(fs: &FakeFs, plan: &OperationPlan, harness: &Harness) -> JobOutcome {
    undo_with(fs, plan, harness.log.held.clone(), false).0
}

#[test]
fn the_planner_still_renames_and_marks_the_overwrite() {
    let (fs, plan) = moving_over(true);
    assert_eq!(
        (plan.steps, plan.conflicts.len()),
        (
            vec![Step::Rename {
                source: fs.location("src/a.txt"),
                target: fs.location("dst/a.txt"),
            }],
            1
        )
    );
}

#[test]
fn a_same_volume_move_over_an_existing_file_succeeds() {
    for workers in MODES {
        let (fs, _, harness) = moved(workers);
        assert_eq!(
            (
                entry_at(&fs, "dst/a.txt"),
                entry_at(&fs, "src/a.txt"),
                harness.log.done(),
            ),
            (Some(moved_entry()), None, vec![0]),
            "workers {workers}"
        );
    }
}

#[test]
fn the_old_target_of_a_move_is_held_with_the_moved_files_stamp() {
    for workers in MODES {
        let (fs, _, harness) = moved(workers);
        let [held] = harness.log.held.as_slice() else {
            panic!("one held file: {:?}", harness.log.held);
        };
        let folder = held.held.parent().unwrap();
        assert_eq!(
            (
                held.original.clone(),
                folder.parent(),
                folder.name().unwrap().to_text().starts_with(HELD_PREFIX),
                fs.metadata(&held.held).ok(),
                held.placed,
            ),
            (
                fs.location("dst/a.txt"),
                Some(fs.location("dst")),
                true,
                Some(old_entry("a.txt")),
                Some(FileStamp {
                    size: BIG,
                    modified_ms: Some(2_000)
                }),
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn the_old_target_of_a_move_is_journaled_while_it_is_still_in_place() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let mut log = Watching {
            fs: &fs,
            seen: Vec::new(),
        };
        let mut ctx = JobContext {
            ops: &fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        };
        let _ = run_job_parallel(&plan, &[], &mut ctx, workers);
        assert_eq!(log.seen, vec![Some(OLD)], "workers {workers}");
    }
}

#[test]
fn releasing_a_finished_move_removes_the_old_file_and_its_folder() {
    for workers in MODES {
        let (fs, _, harness) = moved(workers);
        assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
        assert_eq!(
            (tree_of(&fs, "dst"), tree_of(&fs, "src")),
            (vec![entry("a.txt", EntryKind::File, BIG)], Vec::new()),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_of_a_move_over_a_file_restores_both_files() {
    for workers in MODES {
        let (fs, plan, harness) = moved(workers);
        assert_eq!(undone(&fs, &plan, &harness), JobOutcome::Done);
        assert_eq!(
            (entry_at(&fs, "src/a.txt"), entry_at(&fs, "dst/a.txt")),
            (Some(moved_entry()), Some(old_entry("a.txt"))),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_then_release_leaves_the_starting_tree() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let _ = undone(&fs, &plan, &harness);
        assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
        assert_eq!(tree_of(&fs, ""), before, "workers {workers}");
    }
}

#[test]
fn a_second_undo_changes_nothing() {
    let (fs, plan, harness) = moved(0);
    let _ = undone(&fs, &plan, &harness);
    let after_first = tree_of(&fs, "");
    assert_eq!(undone(&fs, &plan, &harness), JobOutcome::Done);
    assert_eq!(tree_of(&fs, ""), after_first);
}

fn kept_moved_file(edit: impl Fn(&FakeFs, &mut Vec<HeldFile>)) -> (FakeFs, JobOutcome) {
    let (fs, plan, harness) = moved(0);
    let mut held = harness.log.held.clone();
    edit(&fs, &mut held);
    let (outcome, _) = undo_with(&fs, &plan, held, false);
    (fs, outcome)
}

fn rewrite_target(fs: &FakeFs, bytes: u64) {
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", bytes);
}

#[test]
fn a_moved_file_changed_since_is_kept_and_the_old_file_goes_beside() {
    let (fs, outcome) = kept_moved_file(|fs, _| rewrite_target(fs, 7));
    assert_eq!(
        (
            outcome,
            size(&fs, "dst/a.txt"),
            size(&fs, "dst/a (before copy).txt"),
            exists(&fs, "src/a.txt"),
        ),
        (JobOutcome::Done, Some(7), Some(OLD), false)
    );
}

#[test]
fn a_moved_file_without_a_stamp_is_kept() {
    let (fs, outcome) = kept_moved_file(|_, held| held[0].placed = None);
    assert_eq!(
        (
            outcome,
            size(&fs, "dst/a.txt"),
            size(&fs, "dst/a (before copy).txt"),
            exists(&fs, "src/a.txt"),
        ),
        (JobOutcome::Done, Some(BIG), Some(OLD), false)
    );
}

#[test]
fn a_moved_file_whose_stamp_has_no_time_is_kept() {
    let (fs, outcome) = kept_moved_file(|_, held| {
        held[0].placed = held[0].placed.map(|stamp| FileStamp {
            modified_ms: None,
            ..stamp
        });
    });
    assert_eq!(
        (outcome, size(&fs, "dst/a.txt"), exists(&fs, "src/a.txt")),
        (JobOutcome::Done, Some(BIG), false)
    );
}

#[test]
fn a_source_name_taken_since_keeps_both_moved_and_new_files() {
    let (fs, outcome) = kept_moved_file(|fs, _| fs.add_file("src/a.txt", 5));
    assert_eq!(
        (
            outcome,
            size(&fs, "dst/a.txt"),
            size(&fs, "src/a.txt"),
            size(&fs, "dst/a (before copy).txt"),
        ),
        (JobOutcome::Done, Some(BIG), Some(5), Some(OLD))
    );
}

#[test]
fn a_target_folder_missing_on_another_volume_keeps_the_job_waiting() {
    let (fs, plan) = moving_over(true);
    fs.set_volume("dst", 2);
    let mut harness = keeping(0);
    let _ = harness.run(&fs, &plan, &[]);
    fs.remove("dst");
    let (outcome, _) = undo_with(&fs, &plan, harness.log.held.clone(), false);
    assert_eq!(
        (failure(&outcome), exists(&fs, "src/a.txt")),
        (Some((0, FsErrorKind::NotFound)), false)
    );
}

#[test]
fn a_target_folder_deleted_on_its_own_volume_lets_the_undo_finish() {
    let (fs, plan, harness) = moved(0);
    fs.remove("dst");
    assert_eq!(undone(&fs, &plan, &harness), JobOutcome::Done);
}

#[test]
fn an_unreadable_held_row_leaves_an_overwriting_move_for_the_next_start() {
    let (fs, plan, _) = moved(0);
    let (outcome, log) = undo_with(&fs, &plan, Vec::new(), true);
    assert_eq!(
        (outcome, entry_at(&fs, "dst/a.txt"), log.entries),
        (JobOutcome::Done, Some(moved_entry()), Vec::new())
    );
}

#[test]
fn a_move_without_a_journal_replaces_in_place() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let mut harness = Harness {
            workers,
            log: Log {
                unjournaled: true,
                ..Log::default()
            },
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(
            (
                outcome,
                fs.calls("src/a.txt", FakeOp::Replace),
                entry_at(&fs, "dst/a.txt"),
                holding(&fs),
            ),
            (
                JobOutcome::Done,
                1,
                Some(moved_entry()),
                Vec::<String>::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_whose_final_rename_fails_keeps_both_files_in_place() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let denied = Fault::Fail(FsErrorKind::AccessDenied);
        fs.fault("src/a.txt", FakeOp::Rename, 1, denied);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                failure(&outcome),
                entry_at(&fs, "dst/a.txt"),
                entry_at(&fs, "src/a.txt"),
                harness.log.done(),
            ),
            (
                Some((0, FsErrorKind::AccessDenied)),
                Some(old_entry("a.txt")),
                Some(moved_entry()),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_across_volumes_over_a_file_still_copies_and_removes() {
    for workers in MODES {
        let (fs, plan) = moving_over(false);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                outcome,
                tree_of(&fs, "dst"),
                tree_of(&fs, "src"),
                holding(&fs)
            ),
            (
                JobOutcome::Done,
                vec![entry("a.txt", EntryKind::File, BIG)],
                Vec::new(),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

fn folder_move(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        same_volume: true,
        ..request(fs, OpKind::Move, &["src/b"], "dst")
    })
}

#[test]
fn a_folder_moved_onto_a_folder_is_not_held_or_merged_by_the_rename() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG), ("dst/b/c.txt", OLD)]);
        let plan = folder_move(&fs);
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(
            (
                failure(&outcome).map(|(seq, _)| seq),
                tree_of(&fs, ""),
                harness.log.held.len()
            ),
            (Some(0), before, 0),
            "workers {workers}"
        );
    }
}

#[test]
fn a_folder_moved_onto_a_file_is_not_held() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG), ("dst/b", OLD)]);
        let plan = folder_move(&fs);
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(
            (
                failure(&outcome).map(|(seq, _)| seq),
                tree_of(&fs, ""),
                harness.log.held.len()
            ),
            (Some(0), before, 0),
            "workers {workers}"
        );
    }
}

use super::crashing::crashed;
use super::*;

struct Crashed {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    before: Snapshot,
}

fn crashed_folder_copy() -> Crashed {
    let fs = fixture(&[
        ("src/b/c1.txt", 5_000),
        ("src/b/c2.txt", 5_000),
        ("src/b/c3.txt", 5_000),
    ]);
    fs.add_file("dst/keep.txt", 7);
    let before = tree_of(&fs, "dst");
    let plan = copy_plan(&fs, &["src/b"], "dst");
    let done = crashed(&fs, &plan, "src/b/c3.txt");
    let _ = clean_up(&plan, &done, &fs);
    Crashed {
        fs,
        plan,
        done,
        before,
    }
}

fn rolled_back(crashed: &Crashed) -> (JobOutcome, Harness) {
    let mut harness = Harness::default();
    let outcome = harness.roll_back(&crashed.fs, &crashed.plan, &crashed.done);
    (outcome, harness)
}

#[test]
fn the_crash_happened_after_two_copies() {
    assert_eq!(crashed_folder_copy().done, [0, 1, 2]);
}

#[test]
fn roll_back_restores_the_target_folder() {
    let crashed = crashed_folder_copy();
    let _ = rolled_back(&crashed);
    assert_eq!(tree_of(&crashed.fs, "dst"), crashed.before);
}

#[test]
fn roll_back_ends_done() {
    assert_eq!(rolled_back(&crashed_folder_copy()).0, JobOutcome::Done);
}

#[test]
fn roll_back_logs_each_undone_step_in_reverse() {
    let (_, harness) = rolled_back(&crashed_folder_copy());
    assert_eq!(
        harness.logged(),
        [
            (2, StepOutcome::Done),
            (1, StepOutcome::Done),
            (0, StepOutcome::Done)
        ]
    );
}

#[test]
fn roll_back_keeps_a_made_folder_that_is_not_empty() {
    let crashed = crashed_folder_copy();
    crashed.fs.add_file("dst/b/new.txt", 1);
    let _ = rolled_back(&crashed);
    assert_eq!(
        tree_of(&crashed.fs, "dst"),
        [
            entry("b", EntryKind::Folder, 0),
            entry("b/new.txt", EntryKind::File, 1),
            entry("keep.txt", EntryKind::File, 7),
        ]
    );
}

fn ran(fs: &FakeFs, request: &PlanRequest) -> OperationPlan {
    let plan = plan(request);
    let _ = run(fs, &plan);
    plan
}

#[test]
fn roll_back_renames_back() {
    let fs = fixture(&[("src/a.txt", BIG)]);
    let request = PlanRequest {
        same_volume: true,
        ..request(&fs, OpKind::Move, &["src/a.txt"], "dst")
    };
    let plan = ran(&fs, &request);
    let _ = Harness::default().roll_back(&fs, &plan, &[0]);
    assert_eq!(
        (size(&fs, "src/a.txt"), exists(&fs, "dst/a.txt")),
        (Some(BIG), false)
    );
}

#[test]
fn roll_back_never_restores_a_recycled_item() {
    let fs = fixture(&[("src/a.txt", BIG)]);
    let request = PlanRequest {
        target: None,
        ..request(&fs, OpKind::Recycle, &["src/a.txt"], "dst")
    };
    let plan = ran(&fs, &request);
    let mut harness = Harness::default();
    let _ = harness.roll_back(&fs, &plan, &[0]);
    assert_eq!(
        (harness.logged(), fs.recycled(), exists(&fs, "src/a.txt")),
        (
            vec![(0, StepOutcome::Skipped)],
            vec!["src/a.txt".to_owned()],
            false
        )
    );
}

#[test]
fn roll_back_of_a_move_keeps_copies_whose_source_is_gone() {
    let fs = fixture(&[("src/b/c.txt", BIG)]);
    let plan = ran(&fs, &request(&fs, OpKind::Move, &["src/b"], "dst"));
    let _ = Harness::default().roll_back(&fs, &plan, &all_seqs(&plan));
    assert_eq!(size(&fs, "dst/b/c.txt"), Some(BIG));
}

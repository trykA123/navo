use super::crashing::crashed_with;
use super::*;

struct Crashed {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    placed: Vec<PlacedFile>,
    before: Snapshot,
}

fn crashed_folder_copy(workers: usize) -> Crashed {
    let fs = fixture(&[
        ("src/b/c1.txt", 5_000),
        ("src/b/c2.txt", 5_000),
        ("src/b/c3.txt", 5_000),
    ]);
    fs.add_file("dst/keep.txt", 7);
    let before = tree_of(&fs, "dst");
    let plan = copy_plan(&fs, &["src/b"], "dst");
    let done = crashed_with(&fs, &plan, "src/b/c3.txt", workers);
    let _ = clean_up(&plan, &done, &fs);
    let placed = stamps_of_done_copies(&fs, &plan, &done);
    Crashed {
        fs,
        plan,
        done,
        placed,
        before,
    }
}

fn rolled_back(crashed: &Crashed) -> (JobOutcome, Harness) {
    let mut harness = Harness {
        placed: crashed.placed.clone(),
        ..Harness::default()
    };
    let outcome = harness.roll_back(&crashed.fs, &crashed.plan, &crashed.done);
    (outcome, harness)
}

#[test]
fn the_standard_crash_happened_after_two_copies() {
    assert_eq!(crashed_folder_copy(0).done, [0, 1, 2]);
}

#[test]
fn roll_back_restores_the_target_folder() {
    for workers in MODES {
        let crashed = crashed_folder_copy(workers);
        let _ = rolled_back(&crashed);
        assert_eq!(tree_of(&crashed.fs, "dst"), crashed.before);
    }
}

#[test]
fn roll_back_ends_done() {
    for workers in MODES {
        assert_eq!(
            rolled_back(&crashed_folder_copy(workers)).0,
            JobOutcome::Done
        );
    }
}

#[test]
fn roll_back_logs_each_undone_step_in_reverse() {
    for workers in MODES {
        let crashed = crashed_folder_copy(workers);
        let (_, harness) = rolled_back(&crashed);
        let mut done = crashed.done.clone();
        done.sort_unstable_by(|a, b| b.cmp(a));
        let undone: Vec<(u32, StepOutcome)> = done
            .into_iter()
            .map(|seq| (seq, StepOutcome::Done))
            .collect();
        assert_eq!(harness.logged(), undone);
    }
}

#[test]
fn roll_back_keeps_a_made_folder_that_is_not_empty() {
    for workers in MODES {
        let crashed = crashed_folder_copy(workers);
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
}

fn ran(workers: usize, fs: &FakeFs, request: &PlanRequest) -> OperationPlan {
    let plan = plan(request);
    let _ = run_in(workers, fs, &plan);
    plan
}

#[test]
fn roll_back_leaves_a_move_without_a_recorded_identity_in_place() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", BIG)]);
        let request = PlanRequest {
            same_volume: true,
            ..request(&fs, OpKind::Move, &["src/a.txt"], "dst")
        };
        let plan = ran(workers, &fs, &request);
        let outcome = Harness::default().roll_back(&fs, &plan, &[0]);
        assert!(matches!(
            outcome,
            JobOutcome::Failed { seq: 0, ref error } if error.kind == FsErrorKind::IdentityUnavailable
        ));
        assert_eq!(
            (size(&fs, "src/a.txt"), exists(&fs, "dst/a.txt")),
            (None, true)
        );
    }
}

#[test]
fn roll_back_never_restores_a_recycled_item() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", BIG)]);
        let request = PlanRequest {
            target: None,
            ..request(&fs, OpKind::Recycle, &["src/a.txt"], "dst")
        };
        let plan = ran(workers, &fs, &request);
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
}

#[test]
fn roll_back_of_a_move_keeps_copies_whose_source_is_gone() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = ran(workers, &fs, &request(&fs, OpKind::Move, &["src/b"], "dst"));
        let _ = Harness::default().roll_back(&fs, &plan, &all_seqs(&plan));
        assert_eq!(size(&fs, "dst/b/c.txt"), Some(BIG));
    }
}

#[test]
fn roll_back_keeps_a_copy_target_the_user_replaced_since() {
    for workers in MODES {
        let crashed = crashed_folder_copy(workers);
        crashed.fs.remove("dst/b/c1.txt");
        crashed.fs.add_entry(
            "dst/b",
            FsEntry {
                name: rusty_core::ProviderPath::from_text("c1.txt"),
                kind: EntryKind::File,
                size: 5_000,
                modified_ms: Some(77),
                attributes: Attributes::default(),
            },
        );
        let _ = rolled_back(&crashed);
        assert_eq!(
            size(&crashed.fs, "dst/b/c1.txt"),
            Some(5_000),
            "workers {workers}"
        );
    }
}

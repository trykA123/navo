//! A move is a copy and then a removal; another program may change a source between the two.
//! These tests change the source right before the job removes it.

use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::Location;

use super::delete_races::BeforeRemove;
use super::*;

const REPLACEMENT: u64 = 777;

fn move_plan(fs: &FakeFs) -> OperationPlan {
    plan(&request(fs, OpKind::Move, &["src/b"], "dst"))
}

/// Runs `plan` and calls `change` once, just before the job first removes `at`.
fn run_changing(
    workers: usize,
    fs: &FakeFs,
    (plan, at): (&OperationPlan, &str),
    change: &(dyn Fn() + Sync),
) -> JobOutcome {
    let (at, fired) = (fs.location(at), AtomicBool::new(false));
    let hook = |location: &Location| {
        if *location == at && !fired.swap(true, Ordering::SeqCst) {
            change();
        }
    };
    let ops = BeforeRemove { fs, hook: &hook };
    run_in(workers, &ops, plan).0
}

#[test]
fn a_move_keeps_a_source_file_replaced_after_its_copy() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/c.txt");
            fs.add_file("src/b/c.txt", REPLACEMENT);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &replace);
        let kept = tree_of(&fs, "src").contains(&entry("b/c.txt", EntryKind::File, REPLACEMENT));
        assert!(
            kept,
            "workers {workers}: the replacement was never copied but the move deleted it \
             ({outcome:?}); dst holds {:?}",
            tree_of(&fs, "dst")
        );
    }
}

#[test]
fn a_move_stops_at_a_replaced_source_as_changed_on_disk() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/c.txt");
            fs.add_file("src/b/c.txt", REPLACEMENT);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &replace);
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(kind, Some(FsErrorKind::ChangedOnDisk), "workers {workers}");
    }
}

#[test]
fn a_move_keeps_a_same_size_same_time_replacement_by_its_file_id() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let twin = || {
            fs.remove("src/b/c.txt");
            let twin = FsEntry {
                name: navo_core::ProviderPath::from_text("c.txt"),
                kind: EntryKind::File,
                size: BIG,
                modified_ms: Some(1_000),
                attributes: Attributes::default(),
            };
            fs.add_entry("src/b", twin);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &twin);
        assert!(exists(&fs, "src/b/c.txt"), "workers {workers}: {outcome:?}");
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::ChangedOnDisk)
        );
    }
}

#[test]
fn a_same_volume_move_renames_without_a_copy() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = plan(&PlanRequest {
            same_volume: true,
            ..request(&fs, OpKind::Move, &["src/b"], "dst")
        });
        assert!(
            plan.steps
                .iter()
                .all(|step| !matches!(step, Step::CopyFile { .. } | Step::RemoveFile { .. }))
        );
        let never: &(dyn Fn() + Sync) = &|| panic!("a rename removes nothing");
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), never);
        assert_eq!(outcome, JobOutcome::Done, "workers {workers}");
        assert!(exists(&fs, "dst/b/c.txt") && !exists(&fs, "src/b/c.txt"));
    }
}

#[test]
fn a_move_keeps_a_file_added_to_a_source_folder_after_its_listing() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let add = || fs.add_file("src/b/late.txt", REPLACEMENT);
        let outcome = run_changing(workers, &fs, (&plan, "src/b"), &add);
        assert!(
            exists(&fs, "src/b/late.txt"),
            "workers {workers}: a file added after the listing was deleted ({outcome:?})"
        );
    }
}

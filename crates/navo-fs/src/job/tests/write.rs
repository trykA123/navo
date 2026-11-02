use navo_core::{FileStamp, OperationPlan, Totals, write_plan};

use super::*;

fn stamp() -> FileStamp {
    FileStamp {
        size: 3,
        modified_ms: Some(1_000),
        hash: [0_u8; 32],
    }
}

fn plan() -> OperationPlan {
    let fs = FakeFs::new();
    write_plan(
        &fs.location("dest/a"),
        &fs.location("dest/.a.navo-part"),
        stamp(),
        11,
    )
}

fn recover(
    fs: &dyn ProviderOps,
    plan: &OperationPlan,
    held: &[HeldFile],
    unsure: Option<u32>,
) -> (JobOutcome, Harness) {
    let mut harness = Harness::default();
    let job = Journaled {
        plan,
        done: &[],
        held,
        placed: &[],
        held_unreadable: false,
        unsure,
    };
    let outcome = harness.with(fs, |ctx| roll_back(&job, ctx));
    (outcome, harness)
}

#[test]
fn a_write_counts_its_staged_length() {
    let plan = plan();
    assert_eq!(
        plan.totals,
        Totals {
            files: 1,
            folders: 0,
            bytes: 11
        }
    );
    let progress = Progress::start(&plan);
    assert_eq!((progress.files_total, progress.bytes_total), (1, 11));
    assert_eq!(
        super::super::run::counted(plan.kind, &plan.steps[0]),
        (11, 1)
    );
}

#[test]
fn a_write_step_is_an_overwrite_of_its_target() {
    let fs = FakeFs::new();
    let plan = plan();
    assert!(super::super::copy::overwrites(
        &plan,
        &fs.location("dest/a")
    ));
    assert!(!super::super::copy::overwrites(
        &plan,
        &fs.location("dest/b")
    ));
}

#[test]
fn recovery_of_an_unsure_write_without_a_hold_changes_nothing() {
    let fs = FakeFs::new();
    fs.add_file("dest/.a.navo-part", 11);
    let plan = plan();
    let (outcome, harness) = recover(&fs, &plan, &[], Some(0));
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(harness.log.undone, [(0, StepOutcome::Skipped)]);
    assert!(fs.metadata(&fs.location("dest/a")).is_err());
    assert_eq!(size(&fs, "dest/.a.navo-part"), Some(11));
}

#[test]
fn recovery_of_a_held_write_brings_the_old_file_back() {
    let fs = FakeFs::new();
    fs.add_file("dest/.a.navo-held", 3);
    let plan = plan();
    let held = [HeldFile {
        original: fs.location("dest/a"),
        held: fs.location("dest/.a.navo-held"),
        placed: None,
        volume: None,
        placed_hash: None,
    }];
    let (outcome, harness) = recover(&fs, &plan, &held, None);
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(harness.log.undone, [(0, StepOutcome::Done)]);
    assert_eq!(size(&fs, "dest/a"), Some(3));
    assert!(!exists(&fs, "dest/.a.navo-held"));
}

#[test]
fn recovery_waits_for_unreadable_held_rows_of_a_write() {
    let fs = FakeFs::new();
    fs.add_file("dest/a", 11);
    let plan = plan();
    let mut harness = Harness::default();
    let job = Journaled {
        plan: &plan,
        done: &[0],
        held: &[],
        placed: &[],
        held_unreadable: true,
        unsure: None,
    };
    assert_eq!(
        harness.with(&fs, |ctx| roll_back(&job, ctx)),
        JobOutcome::Done
    );
    assert!(harness.log.undone.is_empty());
    assert_eq!(size(&fs, "dest/a"), Some(11));
}

use super::*;

fn old_target() -> FakeFs {
    let fs = fixture(&[("src/a.txt", BIG), ("src/b.txt", 2_000)]);
    fs.add_file("dst/a.txt", 3);
    fs
}

fn overwrite(fault: Option<Fault>) -> (FakeFs, JobOutcome) {
    let fs = old_target();
    let plan = plan_with(&fs, &["src/a.txt"], ConflictPolicy::OverwriteAll);
    if let Some(fault) = fault {
        fs.fault("src/a.txt", FakeOp::Copy, 1, fault);
    }
    let (outcome, _) = run(&fs, &plan);
    (fs, outcome)
}

fn disk_full() -> Option<Fault> {
    Some(Fault::FailAfterBytes {
        kind: FsErrorKind::DiskFull,
        bytes: 5_000,
    })
}

#[test]
fn overwrite_replaces_the_target() {
    let (fs, outcome) = overwrite(None);
    assert_eq!(
        (outcome, size(&fs, "dst/a.txt"), leftovers(&fs)),
        (JobOutcome::Done, Some(BIG), Vec::new())
    );
}

#[test]
fn a_failed_overwrite_keeps_the_old_target_intact() {
    let (fs, _) = overwrite(disk_full());
    assert_eq!(tree_of(&fs, "dst"), [entry("a.txt", EntryKind::File, 3)]);
}

#[test]
fn a_failed_overwrite_never_removes_the_old_target() {
    let (fs, _) = overwrite(disk_full());
    assert_eq!(fs.calls("dst/a.txt", FakeOp::Remove), 0);
}

#[test]
fn a_failed_overwrite_fails_at_its_step() {
    let (_, outcome) = overwrite(disk_full());
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::DiskFull)));
}

fn ask() -> (FakeFs, JobOutcome, Harness) {
    let fs = old_target();
    let plan = plan_with(&fs, &["src/a.txt", "src/b.txt"], ConflictPolicy::Ask);
    let (outcome, harness) = run(&fs, &plan);
    (fs, outcome, harness)
}

#[test]
fn an_asked_conflict_is_logged_skipped() {
    let (_, _, harness) = ask();
    assert_eq!(
        harness.logged(),
        [(0, StepOutcome::Skipped), (1, StepOutcome::Done)]
    );
}

#[test]
fn an_asked_conflict_keeps_the_existing_target() {
    let (fs, _, _) = ask();
    assert_eq!(size(&fs, "dst/a.txt"), Some(3));
}

#[test]
fn the_job_continues_after_a_skipped_step() {
    let (fs, outcome, _) = ask();
    assert_eq!(
        (outcome, size(&fs, "dst/b.txt"), leftovers(&fs)),
        (JobOutcome::Done, Some(2_000), Vec::new())
    );
}

#[test]
fn a_target_that_appeared_after_planning_is_skipped() {
    let fs = fixture(&[("src/a.txt", BIG)]);
    let plan = copy_plan(&fs, &["src/a.txt"], "dst");
    fs.add_file("dst/a.txt", 3);
    let (_, harness) = run(&fs, &plan);
    assert_eq!(
        (harness.logged(), size(&fs, "dst/a.txt")),
        (vec![(0, StepOutcome::Skipped)], Some(3))
    );
}

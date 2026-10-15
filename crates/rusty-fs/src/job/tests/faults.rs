use super::*;

fn faulted(path: &str, op: FakeOp, fault: Fault) -> (FakeFs, JobOutcome, Harness) {
    let fs = three_files();
    let plan = copy_three(&fs);
    fs.fault(path, op, 1, fault);
    let (outcome, harness) = run(&fs, &plan);
    (fs, outcome, harness)
}

fn after_bytes(kind: FsErrorKind) -> (FakeFs, JobOutcome, Harness) {
    let fault = Fault::FailAfterBytes { kind, bytes: 5_000 };
    faulted("src/two.txt", FakeOp::Copy, fault)
}

fn only_the_first_copy() -> Snapshot {
    vec![entry("one.txt", EntryKind::File, BIG)]
}

#[test]
fn disk_full_mid_copy_fails_at_its_step() {
    let (_, outcome, _) = after_bytes(FsErrorKind::DiskFull);
    assert_eq!(failure(&outcome), Some((1, FsErrorKind::DiskFull)));
}

#[test]
fn disk_full_mid_copy_leaves_no_temporary_or_partial_file() {
    let (fs, _, _) = after_bytes(FsErrorKind::DiskFull);
    assert_eq!(tree_of(&fs, "dst"), only_the_first_copy());
}

#[test]
fn network_lost_mid_copy_fails_at_its_step() {
    let (_, outcome, _) = after_bytes(FsErrorKind::NetworkLost);
    assert_eq!(failure(&outcome), Some((1, FsErrorKind::NetworkLost)));
}

#[test]
fn network_lost_mid_copy_leaves_no_temporary_or_partial_file() {
    let (fs, _, _) = after_bytes(FsErrorKind::NetworkLost);
    assert_eq!(tree_of(&fs, "dst"), only_the_first_copy());
}

fn locked() -> (FakeFs, JobOutcome, Harness) {
    faulted(
        "src/two.txt",
        FakeOp::Copy,
        Fault::Fail(FsErrorKind::Locked),
    )
}

#[test]
fn a_locked_source_fails_at_its_step() {
    let (_, outcome, _) = locked();
    assert_eq!(failure(&outcome), Some((1, FsErrorKind::Locked)));
}

#[test]
fn a_locked_source_leaves_no_temporary_or_partial_file() {
    let (fs, _, _) = locked();
    assert_eq!(tree_of(&fs, "dst"), only_the_first_copy());
}

#[test]
fn a_failed_step_is_logged_failed_and_later_steps_are_not_logged() {
    let (_, _, harness) = after_bytes(FsErrorKind::DiskFull);
    assert_eq!(
        harness.logged(),
        [(0, StepOutcome::Done), (1, StepOutcome::Failed)]
    );
}

#[test]
fn a_failed_step_stops_the_job() {
    let (fs, _, _) = after_bytes(FsErrorKind::DiskFull);
    assert_eq!(fs.calls("src/three.txt", FakeOp::Copy), 0);
}

fn denied_folder() -> (FakeFs, JobOutcome) {
    let fs = fixture(&[("src/b/c.txt", BIG)]);
    let plan = copy_plan(&fs, &["src/b"], "dst");
    fs.fault(
        "dst/b",
        FakeOp::Create,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let (outcome, _) = run(&fs, &plan);
    (fs, outcome)
}

#[test]
fn access_denied_on_create_folder_fails_at_its_step() {
    let (_, outcome) = denied_folder();
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::AccessDenied)));
}

#[test]
fn access_denied_on_create_folder_writes_nothing() {
    let (fs, _) = denied_folder();
    assert_eq!(tree_of(&fs, "dst"), Snapshot::new());
}

fn shrunk_source() -> (FakeFs, JobOutcome) {
    let fs = three_files();
    let plan = copy_three(&fs);
    fs.add_file("src/two.txt", 5_999);
    let (outcome, _) = run(&fs, &plan);
    (fs, outcome)
}

#[test]
fn a_copy_that_does_not_match_the_planned_size_fails() {
    let (_, outcome) = shrunk_source();
    assert_eq!(failure(&outcome).map(|(seq, _)| seq), Some(1));
}

#[test]
fn a_copy_that_does_not_match_the_planned_size_leaves_nothing() {
    let (fs, _) = shrunk_source();
    assert_eq!(tree_of(&fs, "dst"), only_the_first_copy());
}

use super::crashing::crashed;
use super::*;

fn crash_on_two() -> (FakeFs, OperationPlan, Vec<u32>) {
    let fs = three_files();
    let plan = copy_three(&fs);
    let done = crashed(&fs, &plan, "src/two.txt");
    (fs, plan, done)
}

fn clean_run() -> Snapshot {
    let fs = three_files();
    let _ = run(&fs, &copy_three(&fs));
    tree_of(&fs, "")
}

fn resumed(clean_first: bool) -> (FakeFs, JobOutcome, Harness) {
    let (fs, plan, done) = crash_on_two();
    if clean_first {
        let _ = clean_up(&plan, &done, &fs);
    }
    let mut harness = Harness::default();
    let outcome = harness.run(&fs, &plan, &done);
    (fs, outcome, harness)
}

#[test]
fn a_crash_records_only_the_steps_done_before_it() {
    assert_eq!(crash_on_two().2, [0]);
}

#[test]
fn a_crash_mid_copy_leaves_a_partial_temporary_file() {
    let (fs, _, _) = crash_on_two();
    assert_eq!(size(&fs, &temp("dst/two.txt")), Some(4096));
}

#[test]
fn a_crash_mid_copy_leaves_nothing_at_the_target() {
    let (fs, _, _) = crash_on_two();
    assert!(!exists(&fs, "dst/two.txt"));
}

#[test]
fn clean_up_removes_the_partial_temporary_file() {
    let (fs, plan, done) = crash_on_two();
    assert_eq!(
        (clean_up(&plan, &done, &fs), leftovers(&fs)),
        (Ok(1), Vec::new())
    );
}

#[test]
fn clean_up_with_nothing_left_removes_nothing() {
    let fs = three_files();
    assert_eq!(clean_up(&copy_three(&fs), &[], &fs), Ok(0));
}

#[test]
fn a_resumed_job_finishes() {
    assert_eq!(resumed(true).1, JobOutcome::Done);
}

#[test]
fn a_resumed_job_equals_a_clean_run() {
    let (fs, _, _) = resumed(true);
    assert_eq!(tree_of(&fs, ""), clean_run());
}

#[test]
fn a_resumed_job_does_not_redo_done_steps() {
    let (fs, _, _) = resumed(true);
    assert_eq!(fs.calls("src/one.txt", FakeOp::Copy), 1);
}

#[test]
fn a_resumed_job_logs_only_the_remaining_steps() {
    let (_, _, harness) = resumed(true);
    assert_eq!(
        harness.logged(),
        [(1, StepOutcome::Done), (2, StepOutcome::Done)]
    );
}

#[test]
fn a_job_resumed_without_clean_up_replaces_the_leftover() {
    let (fs, outcome, _) = resumed(false);
    assert_eq!(
        (outcome, leftovers(&fs), size(&fs, "dst/two.txt")),
        (JobOutcome::Done, Vec::new(), Some(6_000))
    );
}

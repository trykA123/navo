use super::*;

fn cancelled(setup: impl FnOnce(&mut Harness)) -> (FakeFs, JobOutcome, Harness) {
    let fs = three_files();
    let plan = copy_three(&fs);
    let mut harness = Harness::default();
    setup(&mut harness);
    let outcome = harness.run(&fs, &plan, &[]);
    (fs, outcome, harness)
}

fn before_start() -> (FakeFs, JobOutcome, Harness) {
    cancelled(|h| h.cancel.cancel())
}

fn after_first_chunk() -> (FakeFs, JobOutcome, Harness) {
    cancelled(|h| h.cancel_after_bytes = Some(0))
}

fn during_second_copy() -> (FakeFs, JobOutcome, Harness) {
    cancelled(|h| h.cancel_after_bytes = Some(BIG))
}

#[test]
fn cancel_before_the_start_ends_cancelled() {
    assert_eq!(before_start().1, JobOutcome::Cancelled);
}

#[test]
fn cancel_before_the_start_writes_and_logs_nothing() {
    let (fs, _, harness) = before_start();
    assert_eq!(
        (tree_of(&fs, "dst"), harness.logged()),
        (Vec::new(), Vec::new())
    );
}

#[test]
fn cancel_mid_copy_ends_cancelled() {
    assert_eq!(after_first_chunk().1, JobOutcome::Cancelled);
}

#[test]
fn cancel_mid_copy_leaves_no_temporary_or_partial_file() {
    let (fs, _, _) = after_first_chunk();
    assert_eq!(tree_of(&fs, "dst"), Snapshot::new());
}

#[test]
fn cancel_mid_copy_does_not_log_the_step() {
    let (_, _, harness) = after_first_chunk();
    assert_eq!(harness.logged(), Vec::new());
}

#[test]
fn cancel_mid_copy_runs_no_later_step() {
    let (fs, _, _) = after_first_chunk();
    assert_eq!(fs.calls("src/two.txt", FakeOp::Copy), 0);
}

#[test]
fn cancel_during_the_second_copy_keeps_the_first() {
    let (fs, outcome, harness) = during_second_copy();
    assert_eq!(
        (outcome, tree_of(&fs, "dst"), harness.logged()),
        (
            JobOutcome::Cancelled,
            vec![entry("one.txt", EntryKind::File, BIG)],
            vec![(0, StepOutcome::Done)]
        )
    );
}

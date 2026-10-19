use super::kept_aside::{entry_at, keeping, old_entry, over_old};
use super::*;

fn overwritten() -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = over_old(&["a.txt"]);
    let mut harness = keeping(0);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn collide_on_held(fs: &FakeFs, harness: &Harness) {
    let held = harness.log.held[0].held.path().to_text();
    fs.fault(&held, FakeOp::Rename, 1, Fault::Collide);
}

fn as_old(fs: &FakeFs, path: &str) -> Option<FsEntry> {
    entry_at(fs, path).map(|entry| FsEntry {
        name: old_entry("a.txt").name,
        ..entry
    })
}

#[test]
fn a_file_that_appears_while_the_old_one_is_put_back_is_never_replaced() {
    let (fs, plan, harness) = overwritten();
    fs.remove("dst/a.txt");
    collide_on_held(&fs, &harness);
    let mut undo = Harness {
        held: harness.log.held.clone(),
        ..Harness::default()
    };
    let outcome = undo.roll_back(&fs, &plan, &harness.log.done());
    assert_eq!(
        (
            outcome,
            size(&fs, "dst/a.txt"),
            as_old(&fs, "dst/a (before copy).txt")
        ),
        (JobOutcome::Done, Some(1), Some(old_entry("a.txt")))
    );
}

#[test]
fn a_beside_name_taken_while_the_old_file_moves_makes_it_try_the_next_one() {
    let (fs, plan, harness) = overwritten();
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", 5);
    collide_on_held(&fs, &harness);
    let mut undo = Harness {
        held: harness.log.held.clone(),
        ..Harness::default()
    };
    let outcome = undo.roll_back(&fs, &plan, &harness.log.done());
    assert_eq!(
        (
            outcome,
            size(&fs, "dst/a.txt"),
            size(&fs, "dst/a (before copy).txt"),
            as_old(&fs, "dst/a (before copy 2).txt")
        ),
        (JobOutcome::Done, Some(5), Some(1), Some(old_entry("a.txt")))
    );
}

#[test]
fn release_never_replaces_a_file_that_appears_at_the_free_name() {
    let (fs, _, harness) = overwritten();
    fs.remove("dst/a.txt");
    collide_on_held(&fs, &harness);
    assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
    assert_eq!(
        (
            size(&fs, "dst/a.txt"),
            as_old(&fs, "dst/a (before copy).txt")
        ),
        (Some(1), Some(old_entry("a.txt")))
    );
}

fn failing_after_linking(fs: &FakeFs, harness: &Harness) {
    let held = harness.log.held[0].held.path().to_text();
    let kind = FsErrorKind::AccessDenied;
    fs.fault(&held, FakeOp::Rename, 1, Fault::LinkThenFail(kind));
}

fn failed_undo(fs: &FakeFs, plan: &OperationPlan, harness: &Harness) -> Option<FsErrorKind> {
    let mut undo = Harness {
        held: harness.log.held.clone(),
        ..Harness::default()
    };
    let outcome = undo.roll_back(fs, plan, &harness.log.done());
    failure(&outcome).map(|(_, kind)| kind)
}

#[test]
fn a_beside_rename_that_fails_for_another_reason_stops_at_once() {
    let (fs, plan, harness) = overwritten();
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", 5);
    failing_after_linking(&fs, &harness);
    let held = harness.log.held[0].held.path().to_text();
    assert_eq!(
        (
            failed_undo(&fs, &plan, &harness),
            size(&fs, "dst/a (before copy 2).txt"),
            size(&fs, &held)
        ),
        (Some(FsErrorKind::AccessDenied), None, Some(3))
    );
}

#[test]
fn a_put_back_that_fails_for_another_reason_does_not_go_beside() {
    let (fs, plan, harness) = overwritten();
    fs.remove("dst/a.txt");
    failing_after_linking(&fs, &harness);
    let held = harness.log.held[0].held.path().to_text();
    assert_eq!(
        (
            failed_undo(&fs, &plan, &harness),
            size(&fs, "dst/a (before copy).txt"),
            size(&fs, &held)
        ),
        (Some(FsErrorKind::AccessDenied), None, Some(3))
    );
}

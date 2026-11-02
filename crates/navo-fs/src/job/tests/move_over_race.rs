use super::crashing::Crashing;
use super::kept_aside::{entry_at, holding, keeping, old_entry, over_old};
use super::move_over::{moved_entry, moving_over};
use super::*;

const OLD: u64 = 3;

fn taken_by_another(fs: &FakeFs) {
    fs.fault("src/a.txt", FakeOp::Rename, 1, Fault::Collide);
}

fn sizes(fs: &FakeFs) -> (Option<u64>, Option<u64>) {
    (size(fs, "dst/a.txt"), size(fs, "dst/a (before copy).txt"))
}

#[test]
fn a_target_taken_during_a_move_over_gets_the_old_file_beside_it() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        taken_by_another(&fs);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                failure(&outcome).map(|(seq, _)| seq),
                sizes(&fs),
                entry_at(&fs, "src/a.txt"),
                holding(&fs),
                harness.log.beside,
                harness.log.stranded.len(),
            ),
            (
                Some(0),
                (Some(1), Some(OLD)),
                Some(moved_entry()),
                Vec::<String>::new(),
                vec![fs.location("dst/a (before copy).txt")],
                0
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_over_that_cannot_put_the_old_file_back_keeps_it_held() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        taken_by_another(&fs);
        let refusing = Crashing::inside_holding(&fs, FakeOp::Rename).refusing();
        let mut harness = keeping(workers);
        let outcome = harness.run(&refusing, &plan, &[]);
        let held = harness.log.held[0].held.path().to_text();
        assert_eq!(
            (
                failure(&outcome),
                size(&fs, &held),
                size(&fs, "dst/a.txt"),
                harness.log.stranded.len(),
                harness.log.beside.len(),
            ),
            (
                Some((0, FsErrorKind::AccessDenied)),
                Some(OLD),
                Some(1),
                1,
                0
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_over_a_link_target_refuses_and_holds_nothing() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        fs.remove("dst/a.txt");
        fs.add_entry(
            "dst",
            FsEntry {
                kind: EntryKind::Link,
                ..old_entry("a.txt")
            },
        );
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(
            (
                failure(&outcome).map(|(seq, _)| seq),
                tree_of(&fs, ""),
                harness.log.held.len(),
            ),
            (Some(0), before, 0),
            "workers {workers}"
        );
    }
}

#[test]
fn a_target_taken_during_a_copy_over_gets_the_old_file_beside_it() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let taking = Crashing::taking(&fs, &temp("dst/a.txt"), "dst/a.txt");
        let (outcome, harness) = run_in(workers, &taking, &plan);
        assert_eq!(
            (
                failure(&outcome).map(|(seq, _)| seq),
                sizes(&fs),
                holding(&fs),
                harness.log.beside.len(),
                harness.log.stranded.len(),
            ),
            (Some(0), (Some(1), Some(OLD)), Vec::<String>::new(), 1, 0),
            "workers {workers}"
        );
    }
}

#[test]
fn a_copy_over_that_cannot_put_the_old_file_back_keeps_it_held() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let denied = Fault::Fail(FsErrorKind::AccessDenied);
        fs.fault(&temp("dst/a.txt"), FakeOp::Replace, 1, denied);
        let refusing = Crashing::inside_holding(&fs, FakeOp::Rename).refusing();
        let mut harness = keeping(workers);
        let outcome = harness.run(&refusing, &plan, &[]);
        let held = harness.log.held[0].held.path().to_text();
        assert_eq!(
            (
                failure(&outcome),
                size(&fs, &held),
                harness.log.stranded.len(),
            ),
            (Some((0, FsErrorKind::AccessDenied)), Some(OLD), 1),
            "workers {workers}"
        );
    }
}

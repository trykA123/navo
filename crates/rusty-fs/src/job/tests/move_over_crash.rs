use super::crashing::{Crashing, crashed_keeping};
use super::kept_aside::{entry_at, holding, keeping, old_entry};
use super::move_over::{moved_entry, moving_over};
use super::*;

struct Crashed {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    held: Vec<HeldFile>,
    before: Snapshot,
}

fn crash_at(workers: usize, crash: impl Fn(&FakeFs) -> Crashing<'_>) -> Crashed {
    let (fs, plan) = moving_over(true);
    let before = tree_of(&fs, "");
    let (done, held) = crashed_keeping(&crash(&fs), &plan, workers);
    Crashed {
        fs,
        plan,
        done,
        held,
        before,
    }
}

impl Crashed {
    fn old_is_kept(&self) -> bool {
        let held = self.held.iter().map(|file| file.held.path().to_text());
        std::iter::once("dst/a.txt".to_owned())
            .chain(held)
            .filter_map(|path| entry_at(&self.fs, &path))
            .any(|found| found.size == 3)
    }

    fn moved_is_kept(&self) -> bool {
        ["src/a.txt", "dst/a.txt"]
            .iter()
            .filter_map(|path| entry_at(&self.fs, path))
            .any(|found| found.size == BIG)
    }

    fn recover(&self) -> JobOutcome {
        let _ = clean_up(&self.plan, &self.done, &self.fs);
        let mut next_start = Harness {
            held: self.held.clone(),
            ..Harness::default()
        };
        let outcome = next_start.roll_back(&self.fs, &self.plan, &self.done);
        if outcome == JobOutcome::Done {
            let _ = release_held(&self.held, &self.fs);
        }
        outcome
    }

    fn restored(&self) -> bool {
        tree_of(&self.fs, "") == self.before
            && entry_at(&self.fs, "src/a.txt") == Some(moved_entry())
            && entry_at(&self.fs, "dst/a.txt") == Some(old_entry("a.txt"))
            && holding(&self.fs).is_empty()
    }
}

fn check_crash(crash: impl Fn(&FakeFs) -> Crashing<'_>) {
    for workers in MODES {
        let crashed = crash_at(workers, &crash);
        assert_eq!(crashed.held.len(), 1, "journaled, workers {workers}");
        assert!(crashed.old_is_kept(), "old file, workers {workers}");
        assert!(crashed.moved_is_kept(), "moved file, workers {workers}");
        assert_eq!(crashed.recover(), JobOutcome::Done, "workers {workers}");
        assert!(crashed.restored(), "recovered, workers {workers}");
    }
}

#[test]
fn a_crash_making_the_holding_folder_of_a_move_loses_nothing() {
    check_crash(|fs| Crashing::inside_holding(fs, FakeOp::Create));
}

#[test]
fn a_crash_while_holding_for_a_move_loses_nothing() {
    check_crash(|fs| Crashing::new(fs, "dst/a.txt", FakeOp::Rename));
}

#[test]
fn a_crash_after_the_hold_before_the_rename_into_place_loses_nothing() {
    check_crash(|fs| Crashing::new(fs, "src/a.txt", FakeOp::Rename));
}

#[test]
fn a_crash_after_the_rename_before_the_journal_loses_nothing() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let before = tree_of(&fs, "");
        let mut harness = Harness {
            log: Log {
                fail_on: Some(0),
                ..Log::default()
            },
            ..keeping(workers)
        };
        let _ = harness.run(&fs, &plan, &[]);
        let crashed = Crashed {
            fs,
            plan,
            done: harness.log.done(),
            held: harness.log.held,
            before,
        };
        assert_eq!(
            (
                crashed.done.clone(),
                entry_at(&crashed.fs, "dst/a.txt"),
                crashed.old_is_kept()
            ),
            (Vec::new(), Some(moved_entry()), true),
            "workers {workers}"
        );
        assert_eq!(crashed.recover(), JobOutcome::Done, "workers {workers}");
        assert!(crashed.restored(), "workers {workers}");
    }
}

#[test]
fn a_crash_after_the_journal_before_the_next_start_is_recovered_as_a_finished_move() {
    for workers in MODES {
        let (fs, plan) = moving_over(true);
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        let crashed = Crashed {
            fs,
            plan,
            done: harness.log.done(),
            held: harness.log.held,
            before,
        };
        assert_eq!(crashed.recover(), JobOutcome::Done, "workers {workers}");
        assert!(crashed.restored(), "workers {workers}");
    }
}

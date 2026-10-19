use super::crashing::{Crashing, crashed_keeping};
use super::kept_aside::{entry_at, holding, keeping, old_entry, over_old};
use super::*;

const TARGET: &str = "dst/a.txt";

struct Crashed {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    held: Vec<HeldFile>,
}

impl Crashed {
    fn old_is_kept(&self) -> bool {
        let at = |path: &str| {
            entry_at(&self.fs, path).map(|mut e| {
                e.name = old_entry("a.txt").name;
                e
            })
        };
        let held = self.held.iter().map(|file| file.held.path().to_text());
        std::iter::once(TARGET.to_owned())
            .chain(held)
            .any(|path| at(&path) == Some(old_entry("a.txt")))
    }

    fn target_size(&self) -> Option<u64> {
        size(&self.fs, TARGET)
    }

    fn recover(&self, ops: &dyn ProviderOps) -> JobOutcome {
        let _ = clean_up(&self.plan, &self.done, ops);
        let mut next_start = Harness {
            held: self.held.clone(),
            ..Harness::default()
        };
        let outcome = next_start.roll_back(ops, &self.plan, &self.done);
        if outcome == JobOutcome::Done {
            let _ = release_held(&self.held, ops);
        }
        outcome
    }

    fn recovered(&self) -> (Option<FsEntry>, Vec<String>, Vec<String>) {
        (
            entry_at(&self.fs, TARGET),
            leftovers(&self.fs),
            holding(&self.fs),
        )
    }
}

fn clean() -> (Option<FsEntry>, Vec<String>, Vec<String>) {
    (Some(old_entry("a.txt")), Vec::new(), Vec::new())
}

fn crash_at(workers: usize, crash: impl Fn(&FakeFs) -> Crashing<'_>) -> Crashed {
    let (fs, plan) = over_old(&["a.txt"]);
    let (done, held) = crashed_keeping(&crash(&fs), &plan, workers);
    Crashed {
        fs,
        plan,
        done,
        held,
    }
}

fn check_crash(crash: impl Fn(&FakeFs) -> Crashing<'_>, target: Option<u64>) {
    for workers in MODES {
        let crashed = crash_at(workers, &crash);
        assert!(crashed.old_is_kept(), "workers {workers}");
        assert_eq!(crashed.target_size(), target, "workers {workers}");
        assert_eq!(crashed.recover(&crashed.fs), JobOutcome::Done);
        assert_eq!(crashed.recovered(), clean(), "workers {workers}");
    }
}

#[test]
fn a_crash_making_the_holding_folder_keeps_the_old_target() {
    check_crash(|fs| Crashing::inside_holding(fs, FakeOp::Create), Some(3));
}

#[test]
fn a_crash_while_holding_keeps_the_old_target() {
    check_crash(|fs| Crashing::new(fs, TARGET, FakeOp::Rename), Some(3));
}

#[test]
fn a_crash_between_hold_and_replace_keeps_the_old_file_held_and_recovery_restores_it() {
    check_crash(|fs| Crashing::new(fs, &temp(TARGET), FakeOp::Replace), None);
}

fn journal_lost_after_placing(workers: usize) -> Crashed {
    let (fs, plan) = over_old(&["a.txt"]);
    let mut harness = Harness {
        log: Log {
            fail_on: Some(0),
            ..Log::default()
        },
        ..keeping(workers)
    };
    let _ = harness.run(&fs, &plan, &[]);
    Crashed {
        fs,
        plan,
        done: harness.log.done(),
        held: harness.log.held,
    }
}

#[test]
fn a_crash_after_replace_before_the_journal_keeps_the_old_file_held() {
    for workers in MODES {
        let crashed = journal_lost_after_placing(workers);
        assert_eq!(
            (
                crashed.done.clone(),
                crashed.target_size(),
                crashed.old_is_kept()
            ),
            (Vec::new(), Some(BIG), true),
            "workers {workers}"
        );
    }
}

#[test]
fn recovery_after_a_lost_journal_write_restores_the_old_file() {
    for workers in MODES {
        let crashed = journal_lost_after_placing(workers);
        assert_eq!(crashed.recover(&crashed.fs), JobOutcome::Done);
        assert_eq!(crashed.recovered(), clean(), "workers {workers}");
    }
}

#[test]
fn a_crash_during_undo_is_finished_by_the_next_recovery() {
    for workers in MODES {
        let crashed = journal_lost_after_placing(workers);
        let held = crashed.held[0].held.path().to_text();
        let dying = Crashing::new(&crashed.fs, &held, FakeOp::Replace);
        assert_ne!(crashed.recover(&dying), JobOutcome::Done);
        assert_eq!(
            (crashed.target_size(), crashed.old_is_kept()),
            (Some(BIG), true),
            "workers {workers}"
        );
        assert_eq!(crashed.recover(&crashed.fs), JobOutcome::Done);
        assert_eq!(crashed.recovered(), clean(), "workers {workers}");
    }
}

#[test]
fn a_crash_during_undo_of_a_held_but_unplaced_file_is_finished_by_the_next_recovery() {
    for workers in MODES {
        let crashed = crash_at(workers, |fs| {
            Crashing::new(fs, &temp(TARGET), FakeOp::Replace)
        });
        let held = crashed.held[0].held.path().to_text();
        let dying = Crashing::new(&crashed.fs, &held, FakeOp::Replace);
        assert_ne!(crashed.recover(&dying), JobOutcome::Done);
        assert!(crashed.old_is_kept(), "workers {workers}");
        assert_eq!(crashed.recover(&crashed.fs), JobOutcome::Done);
        assert_eq!(crashed.recovered(), clean(), "workers {workers}");
    }
}

#[test]
fn a_finished_overwrite_undone_twice_stays_restored() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let crashed = Crashed {
            fs,
            plan,
            done: harness.log.done(),
            held: harness.log.held,
        };
        let _ = clean_up(&crashed.plan, &crashed.done, &crashed.fs);
        for _ in 0..2 {
            let mut undo = Harness {
                held: crashed.held.clone(),
                ..Harness::default()
            };
            assert_eq!(
                undo.roll_back(&crashed.fs, &crashed.plan, &crashed.done),
                JobOutcome::Done
            );
        }
        assert_eq!(
            entry_at(&crashed.fs, TARGET),
            Some(old_entry("a.txt")),
            "workers {workers}"
        );
    }
}

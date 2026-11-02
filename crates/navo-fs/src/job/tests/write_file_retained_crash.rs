use super::crashing::Crashing;
use super::write_file::{NEW_HASH, OLD_HASH, STAGED, TARGET, hash_at, saving, seen};
use super::write_file_retained::retaining;
use super::*;

struct Stopped {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    held: Vec<HeldFile>,
    proved: usize,
}

impl Stopped {
    fn restart(&self) -> JobOutcome {
        let mut next_start = Harness {
            held: self.held.clone(),
            ..Harness::default()
        };
        next_start.roll_back(&self.fs, &self.plan, &self.done)
    }

    fn hash_of(&self, path: &str) -> Option<[u8; 32]> {
        exists(&self.fs, path).then(|| hash_at(&self.fs, path))
    }

    fn kept(&self) -> Option<[u8; 32]> {
        self.held
            .first()
            .and_then(|held| self.hash_of(&held.held.path().to_text()))
    }

    fn keeps_both(&self) -> bool {
        let target = self.hash_of(TARGET);
        let old = target == Some(OLD_HASH) || self.kept() == Some(OLD_HASH);
        let new = target == Some(NEW_HASH) || self.hash_of(STAGED) == Some(NEW_HASH);
        old && new
    }
}

fn stopped(fs: FakeFs, plan: OperationPlan, harness: Harness) -> Stopped {
    Stopped {
        fs,
        plan,
        done: harness.log.done(),
        proved: harness.log.proofs.len(),
        held: harness.log.held,
    }
}

fn crash(at: impl Fn(&FakeFs) -> Crashing<'_>, workers: usize) -> Stopped {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = retaining(workers);
    let ops = at(&fs);
    let _ = harness.run(&ops, &plan, &[]);
    assert!(
        ops.is_dead(),
        "the crash point was reached, workers {workers}"
    );
    stopped(fs, plan, harness)
}

fn check_restores_the_old_file(at: impl Fn(&FakeFs) -> Crashing<'_>, journaled: usize) {
    for workers in MODES {
        let stopped = crash(&at, workers);
        assert_eq!(stopped.held.len(), journaled, "workers {workers}");
        assert_eq!(stopped.proved, journaled, "workers {workers}");
        assert!(stopped.done.is_empty(), "workers {workers}");
        assert!(stopped.keeps_both(), "after the crash, workers {workers}");
        assert_eq!(stopped.restart(), JobOutcome::Done, "workers {workers}");
        assert!(stopped.keeps_both(), "after recovery, workers {workers}");
        assert_eq!(stopped.hash_of(TARGET), Some(OLD_HASH), "workers {workers}");
        assert_eq!(stopped.hash_of(STAGED), Some(NEW_HASH), "workers {workers}");
    }
}

#[test]
fn a_crash_making_the_retained_holder_leaves_both_files_and_no_proof() {
    check_restores_the_old_file(|fs| Crashing::inside_holding(fs, FakeOp::Create), 0);
}

#[test]
fn a_crash_hiding_the_retained_holder_recovers_the_old_file() {
    check_restores_the_old_file(|fs| Crashing::inside_holding(fs, FakeOp::Secure), 1);
}

#[test]
fn a_crash_while_holding_the_old_file_under_retention_recovers_it() {
    check_restores_the_old_file(|fs| Crashing::new(fs, TARGET, FakeOp::Rename), 1);
}

#[test]
fn a_crash_after_the_retained_hold_before_placing_recovers_the_old_file() {
    for workers in MODES {
        let stopped = crash(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), workers);
        assert!(!exists(&stopped.fs, TARGET), "workers {workers}");
        assert_eq!(stopped.kept(), Some(OLD_HASH), "workers {workers}");
    }
    check_restores_the_old_file(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), 1);
}

#[test]
fn a_crash_after_placing_a_retained_save_keeps_the_new_file_and_the_held_old_one() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let mut harness = retaining(workers);
        harness.log.fail_on = Some(0);
        let _ = harness.run(&fs, &plan, &[]);
        let stopped = stopped(fs, plan, harness);
        assert!(stopped.done.is_empty(), "workers {workers}");
        assert_eq!(stopped.proved, 1, "workers {workers}");
        assert_eq!(stopped.restart(), JobOutcome::Done, "workers {workers}");
        assert_eq!(stopped.hash_of(TARGET), Some(NEW_HASH), "workers {workers}");
        assert_eq!(stopped.kept(), Some(OLD_HASH), "workers {workers}");
        assert!(!exists(&stopped.fs, STAGED), "workers {workers}");
    }
}

#[test]
fn a_crash_after_a_finished_retained_save_with_an_outside_edit_touches_neither_file() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = retaining(0);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    let stopped = stopped(fs, plan, harness);
    super::write_file::put(&stopped.fs, TARGET, (3, 5_000), [9; 32]);
    assert_eq!(
        failure(&stopped.restart()),
        Some((0, FsErrorKind::ChangedOnDisk))
    );
    assert_eq!(stopped.hash_of(TARGET), Some([9; 32]));
    assert_eq!(stopped.kept(), Some(OLD_HASH));
}

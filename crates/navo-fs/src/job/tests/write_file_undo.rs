use super::crashing::{Crashing, crashed_keeping};
use super::kept_aside::{entry_at, holding, keeping};
use super::write_file::{NEW, NEW_HASH, OLD, OLD_HASH, STAGED, TARGET, hash_at, put, saving, seen};
use super::*;

struct Stopped {
    fs: FakeFs,
    plan: OperationPlan,
    done: Vec<u32>,
    held: Vec<HeldFile>,
}

impl Stopped {
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

    fn has_old(&self) -> bool {
        size(&self.fs, TARGET) == Some(OLD)
            && hash_at(&self.fs, TARGET) == OLD_HASH
            && entry_at(&self.fs, TARGET).and_then(|e| e.modified_ms) == Some(1_000)
    }
}

fn crash(crash: impl Fn(&FakeFs) -> Crashing<'_>, workers: usize) -> Stopped {
    let (fs, plan) = saving(STAGED, seen());
    let (done, held) = crashed_keeping(&crash(&fs), &plan, workers);
    Stopped {
        fs,
        plan,
        done,
        held,
    }
}

fn finished(workers: usize) -> Stopped {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    Stopped {
        fs,
        plan,
        done: harness.log.done(),
        held: harness.log.held,
    }
}

fn check_recovers_the_old_file(at: impl Fn(&FakeFs) -> Crashing<'_>) {
    for workers in MODES {
        let stopped = crash(&at, workers);
        assert_eq!(stopped.held.len(), 1, "journaled, workers {workers}");
        assert!(stopped.done.is_empty(), "workers {workers}");
        assert_eq!(stopped.recover(), JobOutcome::Done, "workers {workers}");
        assert!(stopped.has_old(), "workers {workers}");
        assert_eq!(size(&stopped.fs, STAGED), Some(NEW), "workers {workers}");
        assert!(holding(&stopped.fs).is_empty(), "workers {workers}");
    }
}

#[test]
fn a_crash_making_the_holding_folder_recovers_the_old_file() {
    check_recovers_the_old_file(|fs| Crashing::inside_holding(fs, FakeOp::Create));
}

#[test]
fn a_crash_while_holding_the_old_file_recovers_it() {
    check_recovers_the_old_file(|fs| Crashing::new(fs, TARGET, FakeOp::Rename));
}

#[test]
fn a_crash_after_the_hold_before_the_rename_recovers_the_old_file() {
    for workers in MODES {
        let stopped = crash(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), workers);
        assert!(!exists(&stopped.fs, TARGET), "workers {workers}");
    }
    check_recovers_the_old_file(|fs| Crashing::new(fs, STAGED, FakeOp::Rename));
}

#[test]
fn a_crash_after_the_rename_keeps_the_new_file() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let mut harness = Harness {
            log: Log {
                fail_on: Some(0),
                ..Log::default()
            },
            ..keeping(workers)
        };
        let _ = harness.run(&fs, &plan, &[]);
        let stopped = Stopped {
            fs,
            plan,
            done: harness.log.done(),
            held: harness.log.held,
        };
        assert!(stopped.done.is_empty(), "workers {workers}");
        assert_eq!(stopped.recover(), JobOutcome::Done, "workers {workers}");
        assert_eq!(size(&stopped.fs, TARGET), Some(NEW), "workers {workers}");
        assert!(!exists(&stopped.fs, STAGED), "workers {workers}");
        assert!(holding(&stopped.fs).is_empty(), "workers {workers}");
    }
}

impl Stopped {
    fn restart(&self) -> JobOutcome {
        let mut next_start = Harness {
            held: self.held.clone(),
            ..Harness::default()
        };
        next_start.roll_back(&self.fs, &self.plan, &self.done)
    }

    fn undo(&self) -> (Result<StepOutcome, FsError>, Vec<HeldFile>) {
        let mut harness = Harness::default();
        let mut result = None;
        let _ = harness.with(&self.fs, |ctx| {
            result = Some(undo_write(&self.held[0], ctx));
            JobOutcome::Done
        });
        let result = result.unwrap_or_else(|| Err(FsError::new(FsErrorKind::Unknown)));
        (result, harness.log.held)
    }

    fn old_is_held(&self) -> bool {
        hash_at(&self.fs, &self.held[0].held.path().to_text()) == OLD_HASH
    }
}

#[test]
fn a_crash_after_done_before_the_job_finished_keeps_the_new_file_and_its_hold() {
    for workers in MODES {
        let stopped = finished(workers);
        assert_eq!(stopped.done, [0], "workers {workers}");
        assert_eq!(stopped.restart(), JobOutcome::Done, "workers {workers}");
        assert_eq!(hash_at(&stopped.fs, TARGET), NEW_HASH, "workers {workers}");
        assert!(stopped.old_is_held(), "workers {workers}");
    }
}

#[test]
fn a_crash_after_the_rename_with_an_outside_edit_touches_neither_file() {
    let stopped = finished(0);
    put(&stopped.fs, TARGET, (NEW, 5_000), [9; 32]);
    assert_eq!(
        failure(&stopped.restart()),
        Some((0, FsErrorKind::ChangedOnDisk))
    );
    assert_eq!(hash_at(&stopped.fs, TARGET), [9; 32]);
    assert!(stopped.old_is_held());
}

#[test]
fn a_crash_before_placing_then_an_outside_file_at_the_target_keeps_the_old_file_held() {
    for (stamp, hash) in [((5, 7_000), [9; 32]), ((NEW, 2_000), [9; 32])] {
        let stopped = crash(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), 0);
        put(&stopped.fs, TARGET, stamp, hash);
        assert_eq!(
            failure(&stopped.recover()),
            Some((0, FsErrorKind::ChangedOnDisk)),
            "{stamp:?}"
        );
        assert_eq!(hash_at(&stopped.fs, TARGET), hash, "{stamp:?}");
        assert!(stopped.old_is_held(), "{stamp:?}");
    }
}

fn holder_flushes(stopped: &Stopped) -> u32 {
    let holder = stopped.held[0].held.parent().expect("holding folder");
    stopped.fs.calls(&holder.path().to_text(), FakeOp::Flush)
}

#[test]
fn a_recovery_restore_flushes_both_folders() {
    let stopped = crash(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), 0);
    let before = (
        stopped.fs.calls("dest", FakeOp::Flush),
        holder_flushes(&stopped),
    );
    assert_eq!(stopped.restart(), JobOutcome::Done);
    assert!(stopped.has_old());
    let after = (
        stopped.fs.calls("dest", FakeOp::Flush),
        holder_flushes(&stopped),
    );
    assert_eq!(after, (before.0 + 1, before.1 + 1));
}

#[test]
fn a_recovery_restore_whose_flush_fails_stays_pending() {
    let stopped = crash(|fs| Crashing::new(fs, STAGED, FakeOp::Rename), 0);
    let next = stopped.fs.calls("dest", FakeOp::Flush) + 1;
    let fail = Fault::Fail(FsErrorKind::Unknown);
    stopped.fs.fault("dest", FakeOp::Flush, next, fail);
    let mut next_start = Harness {
        held: stopped.held.clone(),
        ..Harness::default()
    };
    let outcome = next_start.roll_back(&stopped.fs, &stopped.plan, &stopped.done);
    assert_eq!(failure(&outcome).map(|(seq, _)| seq), Some(0));
    assert_eq!(next_start.log.undone, [(0, StepOutcome::Failed)]);
    assert!(stopped.has_old());
}

#[test]
fn undo_restores_the_exact_old_file_and_holds_the_new_one() {
    let stopped = finished(0);
    let (result, held) = stopped.undo();
    assert_eq!(result, Ok(StepOutcome::Done));
    assert!(stopped.has_old());
    let [current] = held.as_slice() else {
        panic!("the replaced new file is held");
    };
    assert_eq!(
        hash_at(&stopped.fs, &current.held.path().to_text()),
        NEW_HASH
    );
    release_held(&held, &stopped.fs).unwrap();
    assert!(stopped.has_old());
}

#[test]
fn undo_is_refused_after_an_outside_edit() {
    for edit in [(NEW, 5_000), (NEW + 1, 2_000), (NEW, 2_000)] {
        let stopped = finished(0);
        put(&stopped.fs, TARGET, edit, [9; 32]);
        let (result, _) = stopped.undo();
        assert_eq!(result.map_err(|e| e.kind), Err(FsErrorKind::ChangedOnDisk));
        assert_eq!(hash_at(&stopped.fs, TARGET), [9; 32], "{edit:?}");
        assert_eq!(entry_at(&stopped.fs, TARGET).map(|e| e.size), Some(edit.0));
        assert!(stopped.old_is_held(), "{edit:?}");
    }
}

#[test]
fn undo_without_a_recorded_hash_is_refused() {
    let mut stopped = finished(0);
    stopped.held[0].placed_hash = None;
    let (result, _) = stopped.undo();
    assert_eq!(result.map_err(|e| e.kind), Err(FsErrorKind::ChangedOnDisk));
    assert_eq!(hash_at(&stopped.fs, TARGET), NEW_HASH);
}

#[test]
fn undo_without_a_recorded_hash_is_refused_even_when_the_target_is_gone() {
    let mut stopped = finished(0);
    stopped.held[0].placed_hash = None;
    stopped.fs.remove(TARGET);
    let (result, _) = stopped.undo();
    assert_eq!(result.map_err(|e| e.kind), Err(FsErrorKind::ChangedOnDisk));
    assert!(!exists(&stopped.fs, TARGET));
    assert!(stopped.old_is_held());
}

#[test]
fn undo_after_the_new_file_was_removed_brings_the_old_one_back() {
    let stopped = finished(0);
    stopped.fs.remove(TARGET);
    let before = (
        stopped.fs.calls("dest", FakeOp::Flush),
        holder_flushes(&stopped),
    );
    assert_eq!(stopped.undo().0, Ok(StepOutcome::Done));
    assert!(stopped.has_old());
    let after = (
        stopped.fs.calls("dest", FakeOp::Flush),
        holder_flushes(&stopped),
    );
    assert_eq!(after, (before.0 + 1, before.1 + 1));
}

#[test]
fn undo_after_the_new_file_was_removed_fails_when_the_flush_fails() {
    let stopped = finished(0);
    stopped.fs.remove(TARGET);
    let next = stopped.fs.calls("dest", FakeOp::Flush) + 1;
    let fail = Fault::Fail(FsErrorKind::Unknown);
    stopped.fs.fault("dest", FakeOp::Flush, next, fail);
    assert_eq!(
        stopped.undo().0.map_err(|e| e.kind),
        Err(FsErrorKind::Unknown)
    );
}

#[test]
fn undo_is_refused_after_the_staged_file_was_rewritten_with_the_same_size_and_time() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = keeping(0);
    let ops = Crashing::meddling(&fs, (TARGET, FakeOp::Rename), |fs| {
        put(fs, STAGED, (NEW, 2_000), [5; 32]);
    });
    assert_eq!(harness.run(&ops, &plan, &[]), JobOutcome::Done);
    let stopped = Stopped {
        fs,
        plan,
        done: harness.log.done(),
        held: harness.log.held,
    };
    let (result, _) = stopped.undo();
    assert_eq!(result.map_err(|e| e.kind), Err(FsErrorKind::ChangedOnDisk));
    assert_eq!(hash_at(&stopped.fs, TARGET), [5; 32]);
    assert!(stopped.old_is_held());
}

// Tests for one job that writes several staged files over their targets.
// They cover the pre-check, a mid-run change, and undo of finished and stopped jobs.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use navo_core::{FileStamp, OperationPlan, WriteTarget, write_many_plan};

use super::crashing::Crashing;
use super::kept_aside::keeping;
use super::write_file::{NEW, NEW_HASH, OLD, OLD_HASH, hash_at, put};
use super::*;

const TARGETS: [&str; 3] = ["dest/a.txt", "dest/b.txt", "dest/c.txt"];
const STAGED: [&str; 3] = ["stage/a.part", "stage/b.part", "stage/c.part"];
const OLD_TIME: i64 = 1_000;
const NEW_TIME: i64 = 2_000;

fn seen() -> FileStamp {
    FileStamp {
        size: OLD,
        modified_ms: Some(OLD_TIME),
        hash: OLD_HASH,
    }
}

fn many() -> (FakeFs, OperationPlan) {
    let fs = FakeFs::new();
    fs.add_folder("dest");
    fs.add_folder("stage");
    let mut writes = Vec::new();
    for (target, staged) in TARGETS.iter().zip(STAGED) {
        put(&fs, target, (OLD, OLD_TIME), OLD_HASH);
        put(&fs, staged, (NEW, NEW_TIME), NEW_HASH);
        writes.push(WriteTarget {
            target: fs.location(target),
            staged: fs.location(staged),
            expect: seen(),
            staged_len: NEW,
        });
    }
    (fs, write_many_plan(&writes).unwrap())
}

fn finished(workers: usize) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = many();
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undo_each(fs: &FakeFs, held: &[HeldFile]) -> Vec<StepOutcome> {
    let mut outcomes = Vec::new();
    for file in held.iter().rev() {
        let mut harness = Harness::default();
        let mut result = None;
        let _ = harness.with(fs, |ctx| {
            result = Some(crate::undo_write(file, ctx));
            JobOutcome::Done
        });
        outcomes.push(result.expect("undo was called").expect("undo works"));
    }
    outcomes
}

#[test]
fn a_three_step_job_writes_all_three_targets() {
    for workers in MODES {
        let (fs, plan, harness) = finished(workers);
        assert_eq!(harness.log.done(), [0, 1, 2], "workers {workers}");
        assert_eq!(harness.log.held.len(), 3, "workers {workers}");
        for (target, staged) in TARGETS.iter().zip(STAGED) {
            assert_eq!(hash_at(&fs, target), NEW_HASH, "workers {workers}");
            assert!(!exists(&fs, staged), "workers {workers}");
        }
        assert!(
            plan.steps
                .iter()
                .all(|step| matches!(step, Step::WriteFile { .. }))
        );
    }
}

#[test]
fn a_changed_third_target_refuses_the_job_before_any_write() {
    for workers in MODES {
        let (fs, plan) = many();
        put(&fs, TARGETS[2], (OLD, OLD_TIME), [9; 32]);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            failure(&outcome),
            Some((2, FsErrorKind::ChangedOnDisk)),
            "workers {workers}"
        );
        for target in &TARGETS[..2] {
            assert_eq!(hash_at(&fs, target), OLD_HASH, "workers {workers}");
        }
        assert_eq!(hash_at(&fs, TARGETS[2]), [9; 32], "workers {workers}");
        for staged in STAGED {
            assert_eq!(size(&fs, staged), Some(NEW), "workers {workers}");
        }
        assert!(harness.log.done().is_empty(), "workers {workers}");
        assert!(harness.log.held.is_empty(), "workers {workers}");
    }
}

#[test]
fn a_target_that_changes_between_the_first_and_second_write_stops_the_job() {
    for workers in MODES {
        let (fs, plan) = many();
        let reads = Arc::new(AtomicUsize::new(0));
        let hits = Arc::clone(&reads);
        let ops = Crashing::meddling(&fs, (TARGETS[1], FakeOp::Read), move |fs| {
            if hits.fetch_add(1, Ordering::SeqCst) > 0 {
                put(fs, TARGETS[1], (OLD, OLD_TIME), [9; 32]);
            }
        });
        let mut harness = keeping(workers);
        let outcome = harness.run(&ops, &plan, &[]);
        assert_eq!(
            failure(&outcome),
            Some((1, FsErrorKind::ChangedOnDisk)),
            "workers {workers}"
        );
        assert_eq!(hash_at(&fs, TARGETS[0]), NEW_HASH, "workers {workers}");
        assert_eq!(hash_at(&fs, TARGETS[1]), [9; 32], "workers {workers}");
        assert_eq!(hash_at(&fs, TARGETS[2]), OLD_HASH, "workers {workers}");
        assert_eq!(harness.log.done(), [0], "workers {workers}");
        assert_eq!(
            undo_each(&fs, &harness.log.held),
            [StepOutcome::Done],
            "workers {workers}"
        );
        assert_eq!(hash_at(&fs, TARGETS[0]), OLD_HASH, "workers {workers}");
    }
}

#[test]
fn undo_of_the_finished_job_restores_all_three_old_files() {
    for workers in MODES {
        let (fs, _plan, harness) = finished(workers);
        assert_eq!(
            undo_each(&fs, &harness.log.held),
            [StepOutcome::Done; 3],
            "workers {workers}"
        );
        for target in TARGETS {
            assert_eq!(hash_at(&fs, target), OLD_HASH, "workers {workers}");
            assert_eq!(size(&fs, target), Some(OLD), "workers {workers}");
            assert_eq!(
                fs.metadata(&fs.location(target)).unwrap().modified_ms,
                Some(OLD_TIME),
                "workers {workers}"
            );
        }
    }
}

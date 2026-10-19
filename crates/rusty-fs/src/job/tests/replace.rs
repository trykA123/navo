use super::crashing::{Crashing, crashed_on};
use super::*;

fn old_target() -> (FakeFs, OperationPlan) {
    let fs = fixture(&[("src/a.txt", BIG)]);
    fs.add_file("dst/a.txt", 3);
    let plan = plan_with(&fs, &["src/a.txt"], ConflictPolicy::OverwriteAll);
    (fs, plan)
}

fn crashed_while_placing(workers: usize) -> (FakeFs, OperationPlan, Vec<u32>) {
    let (fs, plan) = old_target();
    let done = crashed_on(&Crashing::placing(&fs, &temp("dst/a.txt")), &plan, workers);
    (fs, plan, done)
}

#[test]
fn without_a_journal_a_crash_while_placing_keeps_the_old_target() {
    for workers in MODES {
        let (fs, plan) = old_target();
        let mut harness = Harness {
            workers,
            log: Log {
                unjournaled: true,
                ..Log::default()
            },
            ..Harness::default()
        };
        let _ = harness.run(&Crashing::placing(&fs, &temp("dst/a.txt")), &plan, &[]);
        assert_eq!(
            (
                harness.log.done(),
                size(&fs, "dst/a.txt"),
                size(&fs, &temp("dst/a.txt"))
            ),
            (Vec::new(), Some(3), Some(BIG)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_job_resumed_after_a_crash_while_placing_overwrites() {
    for workers in MODES {
        let (fs, plan, done) = crashed_while_placing(workers);
        let _ = clean_up(&plan, &done, &fs);
        let mut harness = Harness {
            workers,
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &done);
        assert_eq!(
            (outcome, size(&fs, "dst/a.txt"), leftovers(&fs)),
            (JobOutcome::Done, Some(BIG), Vec::new()),
            "workers {workers}"
        );
    }
}

#[test]
fn an_overwrite_replaces_the_target_in_one_call() {
    for workers in MODES {
        let (fs, plan) = old_target();
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                outcome,
                fs.calls("dst/a.txt", FakeOp::Remove),
                fs.calls(&temp("dst/a.txt"), FakeOp::Replace),
                size(&fs, "dst/a.txt"),
            ),
            (JobOutcome::Done, 0, 1, Some(BIG)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_failed_replace_keeps_the_old_target_and_logs_nothing_done() {
    for workers in MODES {
        let (fs, plan) = old_target();
        let denied = Fault::Fail(FsErrorKind::AccessDenied);
        fs.fault(&temp("dst/a.txt"), FakeOp::Replace, 1, denied);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                failure(&outcome),
                harness.log.done(),
                size(&fs, "dst/a.txt"),
                leftovers(&fs),
            ),
            (
                Some((0, FsErrorKind::AccessDenied)),
                Vec::new(),
                Some(3),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_copy_to_a_free_target_still_renames() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", BIG)]);
        let plan = copy_plan(&fs, &["src/a.txt"], "dst");
        let _ = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                fs.calls(&temp("dst/a.txt"), FakeOp::Rename),
                fs.calls(&temp("dst/a.txt"), FakeOp::Replace),
            ),
            (1, 0),
            "workers {workers}"
        );
    }
}

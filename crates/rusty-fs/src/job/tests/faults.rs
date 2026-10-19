use super::*;

struct Faulted {
    fs: FakeFs,
    plan: OperationPlan,
    outcome: JobOutcome,
    harness: Harness,
}

fn faulted(workers: usize, path: &str, op: FakeOp, fault: Fault) -> Faulted {
    let fs = three_files();
    let plan = copy_three(&fs);
    fs.fault(path, op, 1, fault);
    let (outcome, harness) = run_in(workers, &fs, &plan);
    Faulted {
        fs,
        plan,
        outcome,
        harness,
    }
}

fn after_bytes(workers: usize, kind: FsErrorKind) -> Faulted {
    let fault = Fault::FailAfterBytes { kind, bytes: 5_000 };
    faulted(workers, "src/two.txt", FakeOp::Copy, fault)
}

fn locked(workers: usize) -> Faulted {
    let fault = Fault::Fail(FsErrorKind::Locked);
    faulted(workers, "src/two.txt", FakeOp::Copy, fault)
}

fn leaves_only_journalled_copies(run: &Faulted) -> bool {
    let done = run.harness.log.done();
    leftovers(&run.fs).is_empty()
        && !exists(&run.fs, "dst/two.txt")
        && done.contains(&0)
        && placed_exactly_the_done_copies(&run.fs, &run.plan, &done)
}

#[test]
fn disk_full_mid_copy_fails_at_its_step() {
    for workers in MODES {
        let run = after_bytes(workers, FsErrorKind::DiskFull);
        assert_eq!(
            failure(&run.outcome),
            Some((1, FsErrorKind::DiskFull)),
            "{workers} workers"
        );
    }
}

#[test]
fn disk_full_mid_copy_leaves_no_temporary_or_partial_file() {
    for workers in MODES {
        let run = after_bytes(workers, FsErrorKind::DiskFull);
        assert!(leaves_only_journalled_copies(&run), "{workers} workers");
    }
}

#[test]
fn network_lost_mid_copy_fails_at_its_step() {
    for workers in MODES {
        let run = after_bytes(workers, FsErrorKind::NetworkLost);
        assert_eq!(
            failure(&run.outcome),
            Some((1, FsErrorKind::NetworkLost)),
            "{workers} workers"
        );
    }
}

#[test]
fn network_lost_mid_copy_leaves_no_temporary_or_partial_file() {
    for workers in MODES {
        let run = after_bytes(workers, FsErrorKind::NetworkLost);
        assert!(leaves_only_journalled_copies(&run), "{workers} workers");
    }
}

#[test]
fn a_locked_source_fails_at_its_step() {
    for workers in MODES {
        assert_eq!(
            failure(&locked(workers).outcome),
            Some((1, FsErrorKind::Locked)),
            "{workers} workers"
        );
    }
}

#[test]
fn a_locked_source_leaves_no_temporary_or_partial_file() {
    for workers in MODES {
        assert!(
            leaves_only_journalled_copies(&locked(workers)),
            "{workers} workers"
        );
    }
}

#[test]
fn a_failed_step_is_logged_failed() {
    for workers in MODES {
        let run = after_bytes(workers, FsErrorKind::DiskFull);
        assert!(
            run.harness.logged().contains(&(1, StepOutcome::Failed)),
            "{workers} workers"
        );
    }
}

#[test]
fn a_failed_standard_step_logs_no_later_step() {
    let run = after_bytes(0, FsErrorKind::DiskFull);
    assert_eq!(
        run.harness.logged(),
        [(0, StepOutcome::Done), (1, StepOutcome::Failed)]
    );
}

#[test]
fn a_failed_standard_step_stops_the_job() {
    let run = after_bytes(0, FsErrorKind::DiskFull);
    assert_eq!(run.fs.calls("src/three.txt", FakeOp::Copy), 0);
}

fn denied_folder(workers: usize) -> (FakeFs, JobOutcome) {
    let fs = fixture(&[("src/b/c.txt", BIG)]);
    let plan = copy_plan(&fs, &["src/b"], "dst");
    fs.fault(
        "dst/b",
        FakeOp::Create,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let (outcome, _) = run_in(workers, &fs, &plan);
    (fs, outcome)
}

#[test]
fn access_denied_on_create_folder_fails_at_its_step() {
    for workers in MODES {
        assert_eq!(
            failure(&denied_folder(workers).1),
            Some((0, FsErrorKind::AccessDenied)),
            "{workers} workers"
        );
    }
}

#[test]
fn access_denied_on_create_folder_writes_nothing() {
    for workers in MODES {
        let (fs, _) = denied_folder(workers);
        assert_eq!(tree_of(&fs, "dst"), Snapshot::new(), "{workers} workers");
    }
}

fn shrunk_source(workers: usize) -> Faulted {
    let fs = three_files();
    let plan = copy_three(&fs);
    fs.add_file("src/two.txt", 5_999);
    let (outcome, harness) = run_in(workers, &fs, &plan);
    Faulted {
        fs,
        plan,
        outcome,
        harness,
    }
}

#[test]
fn a_copy_that_does_not_match_the_planned_size_fails() {
    for workers in MODES {
        assert_eq!(
            failure(&shrunk_source(workers).outcome).map(|(seq, _)| seq),
            Some(1),
            "{workers} workers"
        );
    }
}

#[test]
fn a_copy_that_does_not_match_the_planned_size_leaves_nothing() {
    for workers in MODES {
        assert!(
            leaves_only_journalled_copies(&shrunk_source(workers)),
            "{workers} workers"
        );
    }
}

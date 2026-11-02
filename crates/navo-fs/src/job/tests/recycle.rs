use super::*;

const SOURCES: [&str; 9] = [
    "src/one.txt",
    "src/two.txt",
    "src/three.txt",
    "src/four.txt",
    "src/five.txt",
    "src/six.txt",
    "src/seven.txt",
    "src/eight.txt",
    "src/nine.txt",
];

fn nine_files() -> FakeFs {
    fixture(&[
        ("src/one.txt", 1),
        ("src/two.txt", 2),
        ("src/three.txt", 3),
        ("src/four.txt", 4),
        ("src/five.txt", 5),
        ("src/six.txt", 6),
        ("src/seven.txt", 7),
        ("src/eight.txt", 8),
        ("src/nine.txt", 9),
    ])
}

fn recycle_nine(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        target: None,
        existing: Vec::new(),
        ..request(fs, OpKind::Recycle, &SOURCES, "dst")
    })
}

fn failed_recycle(workers: usize) -> (FakeFs, OperationPlan, JobOutcome, Harness) {
    let fs = nine_files();
    let plan = recycle_nine(&fs);
    fs.fault(
        "src/five.txt",
        FakeOp::Recycle,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let (outcome, harness) = run_in(workers, &fs, &plan);
    (fs, plan, outcome, harness)
}

fn expected_log() -> Vec<(u32, StepOutcome)> {
    (0_u32..9)
        .map(|seq| {
            (
                seq,
                if seq == 4 {
                    StepOutcome::Failed
                } else {
                    StepOutcome::Done
                },
            )
        })
        .collect()
}

#[test]
fn a_failed_recycle_keeps_going_and_bins_the_rest() {
    for workers in MODES {
        let (fs, plan, outcome, harness) = failed_recycle(workers);
        assert_eq!(plan.steps.len(), 9);
        assert_eq!(
            failure(&outcome),
            Some((4, FsErrorKind::AccessDenied)),
            "{workers} workers"
        );
        let binned: Vec<String> = SOURCES
            .iter()
            .enumerate()
            .filter(|(seq, _)| *seq != 4)
            .map(|(_, path)| (*path).to_owned())
            .collect();
        assert_eq!(fs.recycled(), binned, "{workers} workers");
        assert_eq!(harness.logged(), expected_log(), "{workers} workers");
    }
}

#[test]
fn the_reported_failure_count_is_the_items_that_failed() {
    for workers in MODES {
        let (_, _, outcome, harness) = failed_recycle(workers);
        assert!(matches!(outcome, JobOutcome::Failed { .. }));
        let progress = harness.progress.last().expect("a progress report");
        assert_eq!(
            (progress.files_done, progress.files_total),
            (8, 9),
            "{workers} workers"
        );
    }
}

#[test]
fn a_cancelled_recycle_after_a_failure_returns_cancelled() {
    for workers in MODES {
        let fs = nine_files();
        let plan = recycle_nine(&fs);
        fs.fault(
            "src/one.txt",
            FakeOp::Recycle,
            1,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
        let mut harness = Harness {
            workers,
            cancel_after_files: Some(1),
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(outcome, JobOutcome::Cancelled, "{workers} workers");
        assert_eq!(
            harness.logged(),
            vec![(0, StepOutcome::Failed), (1, StepOutcome::Done)],
            "{workers} workers"
        );
        assert_eq!(fs.calls("src/three.txt", FakeOp::Recycle), 0);
        assert_eq!(fs.recycled(), vec!["src/two.txt".to_owned()]);
        assert_eq!(
            harness
                .progress
                .last()
                .map(|progress| (progress.files_done, progress.files_total)),
            Some((1, 9)),
            "{workers} workers"
        );
    }
}

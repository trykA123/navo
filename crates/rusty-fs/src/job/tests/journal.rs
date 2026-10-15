use super::*;

fn failing_log() -> (FakeFs, JobOutcome) {
    let fs = three_files();
    let plan = copy_three(&fs);
    let mut harness = Harness {
        log: Log {
            fail_on: Some(1),
            ..Log::default()
        },
        ..Harness::default()
    };
    let outcome = harness.run(&fs, &plan, &[]);
    (fs, outcome)
}

#[test]
fn a_journal_write_failure_stops_the_job_at_that_step() {
    assert_eq!(
        failing_log().1,
        JobOutcome::Failed {
            seq: 1,
            error: log_error()
        }
    );
}

#[test]
fn a_journal_write_failure_runs_no_later_step() {
    let (fs, _) = failing_log();
    assert_eq!(fs.calls("src/three.txt", FakeOp::Copy), 0);
}

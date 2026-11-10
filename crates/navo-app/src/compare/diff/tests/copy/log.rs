//! A copy whose job ran to Done but whose log cannot finish still reports Copied (VR2 lane A).

use navo_core::FsError;
use navo_fs::{HeldFile, JobContext, JobLog, JobOutcome, ProviderOps, StepOutcome, run_job};
use navo_ui::browser::{LoggedOperation, OpLog};

use super::*;

/// Runs the job through the real compare log, then fails its finish like a full journal disk.
struct FinishFails {
    inner: Box<dyn OpLog>,
}

impl JobLog for FinishFails {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        unreachable!("the inner log records the steps")
    }

    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        unreachable!("a copy never undoes")
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        unreachable!("the inner log records held files")
    }
}

impl OpLog for FinishFails {
    fn check(&mut self, operation: LoggedOperation<'_>) -> Result<(), String> {
        self.inner.check(operation)
    }

    fn execute(&mut self, operation: LoggedOperation<'_>) -> Option<JobOutcome> {
        Some(run_job(
            operation.plan,
            &[],
            &mut JobContext {
                ops: operation.ops,
                log: self.inner.as_mut(),
                cancel: operation.cancel,
                progress: operation.progress,
            },
        ))
    }

    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps) -> Result<(), String> {
        self.inner.finish(outcome, ops)?;
        Err("journal write failed".to_owned())
    }
}

#[test]
fn a_written_copy_whose_log_cannot_finish_still_reports_copied() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    let inner = Arc::clone(&fixture.factory);
    let failing: LogFactory = Arc::new(move |plan| {
        let log: Box<dyn OpLog> = Box::new(FinishFails {
            inner: inner(plan)?,
        });
        Ok(log)
    });
    assert_eq!(fixture.copy_with(&request, &failing), Ok(()));
    assert_eq!(fixture.read("right.txt"), LEFT);
}

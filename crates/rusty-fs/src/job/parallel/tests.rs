use super::*;
use crate::{CancelToken, FakeFs, HeldFile, JobLog};
use std::cell::RefCell;

struct Log;
impl JobLog for Log {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }
    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }
    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

#[test]
fn a_pause_requested_by_the_resume_report_is_reported_before_waiting_for_workers() {
    let fs = FakeFs::new();
    let plan = OperationPlan {
        kind: rusty_core::OpKind::Copy,
        steps: vec![],
        totals: rusty_core::Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    };
    let cancel = CancelToken::new();
    let observed = RefCell::new(Vec::new());
    let mut report = |progress: &Progress| {
        observed.borrow_mut().push(progress.paused);
        if !progress.paused {
            cancel.pause();
        }
    };
    let mut ctx = JobContext {
        ops: &fs,
        log: &mut Log,
        cancel: &cancel,
        progress: &mut report,
    };
    let aside = Aside::new();
    let removals = Removals::of(&plan);
    let (tasks, _queue) = channel();
    let (_sender, events) = channel();
    let pool = Pool {
        tasks,
        events,
        workers: 1,
    };
    let mut runner = Runner::new(
        Job {
            plan: &plan,
            aside: &aside,
            namespace: None,
            removals: &removals,
        },
        &mut ctx,
        pool,
    );
    runner.progress.paused = true;
    runner.note_pause();
    assert_eq!(*observed.borrow(), vec![false, true]);
    assert!(runner.progress.paused);
}

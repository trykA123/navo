mod copy;
mod run;
mod undo;

use rusty_core::{FsError, Location, OperationPlan};

use crate::{CancelToken, ProviderOps};

pub const TEMP_SUFFIX: &str = ".ferrite-part";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Done,
    Skipped,
    Failed,
}

pub trait JobLog {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    Done,
    Cancelled,
    Failed { seq: u32, error: FsError },
}

pub struct JobContext<'a> {
    pub ops: &'a dyn ProviderOps,
    pub log: &'a mut dyn JobLog,
    pub cancel: &'a CancelToken,
    pub progress: &'a mut dyn FnMut(&Progress),
}

#[must_use]
pub fn run_job(plan: &OperationPlan, done: &[u32], ctx: &mut JobContext<'_>) -> JobOutcome {
    run::run(plan, done, ctx)
}

#[must_use]
pub fn roll_back(plan: &OperationPlan, done: &[u32], ctx: &mut JobContext<'_>) -> JobOutcome {
    undo::roll_back(plan, done, ctx)
}

pub fn clean_up(plan: &OperationPlan, done: &[u32], ops: &dyn ProviderOps) -> Result<u32, FsError> {
    undo::clean_up(plan, done, ops)
}

#[cfg(test)]
mod tests;

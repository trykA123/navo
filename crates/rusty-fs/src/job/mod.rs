mod aside;
mod copy;
mod hold;
mod parallel;
mod pool;
mod rename;
mod restore;
mod run;
mod undo;
mod workers;

use rusty_core::{FsError, Location, OperationPlan};

use crate::{CancelToken, ProviderOps};

pub use aside::HELD_PREFIX;
pub use rename::RenameOutcome;
pub use workers::{CopyMode, WorkerCounts, copy_workers, job_workers};

pub use rusty_core::TEMP_SUFFIX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldFile {
    pub original: Location,
    pub held: Location,
    pub placed: Option<FileStamp>,
    pub volume: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedFile {
    pub target: Location,
    pub stamp: FileStamp,
}

pub trait JobLog {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError>;
    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError>;
    fn journal_placed(&mut self, _placed: &PlacedFile) -> Result<(), FsError> {
        Ok(())
    }
    fn kept_beside(&mut self, _held: &HeldFile, _beside: &Location) {}
    fn held_stranded(&mut self, _held: &HeldFile) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: Option<Location>,
    pub paused: bool,
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
pub fn run_job_parallel(
    plan: &OperationPlan,
    done: &[u32],
    ctx: &mut JobContext<'_>,
    workers: usize,
) -> JobOutcome {
    if workers <= 1 {
        return run::run(plan, done, ctx);
    }
    parallel::run(plan, done, ctx, workers)
}

#[must_use]
pub fn run_rename(plan: &OperationPlan, ctx: &mut JobContext<'_>) -> RenameOutcome {
    rename::run_rename(plan, ctx)
}

pub struct Journaled<'a> {
    pub plan: &'a OperationPlan,
    pub done: &'a [u32],
    pub held: &'a [HeldFile],
    pub placed: &'a [PlacedFile],
    pub held_unreadable: bool,
}

#[must_use]
pub fn roll_back(job: &Journaled<'_>, ctx: &mut JobContext<'_>) -> JobOutcome {
    undo::roll_back(job, ctx)
}

pub fn release_held(held: &[HeldFile], ops: &dyn ProviderOps) -> Result<(), FsError> {
    restore::release(held, ops)
}

pub fn clean_up(plan: &OperationPlan, done: &[u32], ops: &dyn ProviderOps) -> Result<u32, FsError> {
    undo::clean_up(plan, done, ops)
}

#[cfg(test)]
mod tests;

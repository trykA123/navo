mod aside;
mod copy;
mod hold;
mod link;
mod marks;
mod parallel;
mod pool;
mod remove;
mod rename;
mod restore;
mod run;
mod temporary;
mod undo;
mod workers;

use rusty_core::{FsError, Location, OpKind, OperationPlan};

use crate::{CancelToken, FileIdentity, ProviderOps};

pub use aside::HELD_PREFIX;
pub use rename::{RenameOutcome, is_temporary as is_rename_temporary};
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
    fn preserve_overwrites(&self) -> bool {
        false
    }
    fn temporary_namespace(&self) -> Option<String> {
        None
    }
    fn prepare_temporary(
        &mut self,
        _proof: &rusty_core::retention::CopyTemporary,
    ) -> Result<(), FsError> {
        Err(FsError::new(rusty_core::FsErrorKind::NotSupportedYet))
    }
    fn created_temporary(
        &mut self,
        _created: (u32, FileIdentity),
        _ops: &dyn ProviderOps,
    ) -> Result<(), FsError> {
        Err(FsError::new(rusty_core::FsErrorKind::NotSupportedYet))
    }
    fn retire_temporary(&mut self, _seq: u32, _ops: &dyn ProviderOps) -> Result<(), FsError> {
        Err(FsError::new(rusty_core::FsErrorKind::NotSupportedYet))
    }
    fn journal_proved_held(
        &mut self,
        _held: &HeldFile,
        _proof: &rusty_core::retention::OverwriteProof,
    ) -> Result<bool, FsError> {
        Err(FsError::new(rusty_core::FsErrorKind::NotSupportedYet))
    }
    fn prepare_rename(&mut self, _seq: u32, _identity: FileIdentity) -> Result<(), FsError> {
        Ok(())
    }
    fn rename_identity(&self, _seq: u32) -> Result<Option<FileIdentity>, FsError> {
        Ok(None)
    }
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError>;
    fn steps(&mut self, marks: &[(u32, StepOutcome)]) -> Result<(), FsError> {
        marks
            .iter()
            .try_for_each(|&(seq, outcome)| self.step(seq, outcome))
    }
    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError>;
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

impl Progress {
    pub fn start(plan: &OperationPlan) -> Self {
        Self {
            bytes_done: 0,
            bytes_total: run::moved_bytes(plan),
            files_done: 0,
            files_total: run::progress_files(plan),
            current: None,
            paused: false,
        }
    }

    pub fn finished(plan: &OperationPlan) -> Self {
        let start = Self::start(plan);
        Self {
            bytes_done: start.bytes_total,
            files_done: start.files_total,
            ..start
        }
    }
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
    if workers <= 1 || plan.kind == OpKind::Rename {
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
    pub unsure: Option<u32>,
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

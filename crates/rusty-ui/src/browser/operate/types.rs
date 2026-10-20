use rusty_core::{FsError, Location, OpKind, OperationPlan, Risk, TabId};
use rusty_fs::{
    CancelToken, CopyMode, DriveClass, HeldFile, JobLog, JobOutcome, Progress, ProviderOps,
    StepOutcome, drive_class,
};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DropRequest {
    pub kind: OpKind,
    pub sources: Vec<Location>,
    pub target: Location,
    pub confirm: DropConfirm,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DropConfirm {
    #[default]
    Never,
    Moves,
    Always,
}

pub trait OpLog: JobLog + Send {
    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps);
}

pub(in crate::browser) type ClassOf = fn(&Location) -> DriveClass;

#[derive(Clone, Copy)]
pub(in crate::browser) struct Copying {
    pub mode: CopyMode,
    pub class_of: ClassOf,
}

impl Default for Copying {
    fn default() -> Self {
        Self {
            mode: CopyMode::default(),
            class_of: drive_class,
        }
    }
}

pub type LogFactory = Arc<dyn Fn(&OperationPlan) -> Result<Box<dyn OpLog>, String> + Send + Sync>;

#[derive(Clone)]
pub struct Operations {
    pub log: LogFactory,
    pub protected: Vec<Location>,
}

impl Operations {
    #[must_use]
    pub fn unlogged() -> Self {
        Self {
            log: Arc::new(|_| Ok(Box::new(NoLog))),
            protected: Vec::new(),
        }
    }
}

struct NoLog;

impl JobLog for NoLog {
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

impl OpLog for NoLog {
    fn finish(&mut self, _: &JobOutcome, _: &dyn ProviderOps) {}
}

#[derive(Debug, Clone)]
pub struct PendingOp {
    pub id: u64,
    pub sources: Vec<Location>,
    pub target: Option<Location>,
    pub plan: OperationPlan,
    pub(crate) source_tab: TabId,
}

impl PendingOp {
    #[must_use]
    pub fn is_protected(&self) -> bool {
        self.plan.risks.iter().any(
            |risk| matches!(risk, Risk::Protected(place) if Some(place) != self.target.as_ref()),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct JobView {
    pub id: u64,
    pub kind: OpKind,
    pub from: Option<Location>,
    pub target: Option<Location>,
    pub sources: usize,
    pub progress: Progress,
    pub status: JobStatus,
    pub(crate) cancel: CancelToken,
    pub(crate) source_tab: TabId,
}

pub(in crate::browser) enum JobUpdate {
    Planned {
        request: OpRequest,
        result: Result<OperationPlan, FsError>,
    },
    Progress {
        id: u64,
        progress: Progress,
    },
    Done {
        id: u64,
        progress: Option<Progress>,
        outcome: Result<JobOutcome, String>,
    },
}

pub(in crate::browser) struct OpRequest {
    pub(super) id: u64,
    pub(super) kind: OpKind,
    pub(super) sources: Vec<Location>,
    pub(super) target: Option<Location>,
    pub(super) same_volume: bool,
    pub(super) protected: Vec<Location>,
    pub(super) source_tab: TabId,
    pub(super) asks: bool,
}

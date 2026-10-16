use rusty_core::{FsError, Location, OpKind, OperationPlan, TabId};
use rusty_fs::{CancelToken, JobLog, JobOutcome, Progress, StepOutcome};

use std::sync::Arc;

pub trait OpLog: JobLog + Send {
    fn finish(&mut self, outcome: &JobOutcome);
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
}

impl OpLog for NoLog {
    fn finish(&mut self, _: &JobOutcome) {}
}

#[derive(Debug, Clone)]
pub struct PendingOp {
    pub id: u64,
    pub sources: Vec<Location>,
    pub target: Option<Location>,
    pub plan: OperationPlan,
    pub(crate) source_tab: TabId,
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
}

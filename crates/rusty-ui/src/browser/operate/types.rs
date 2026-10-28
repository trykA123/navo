use rusty_core::retention::RetentionPolicy;
use rusty_core::{FsError, Location, OpKind, OperationPlan, Risk, TabId};
use rusty_fs::{
    CancelToken, CopyMode, DriveClass, HeldFile, JobLog, JobOutcome, Progress, ProviderOps,
    StepOutcome, drive_class,
};

use std::sync::Arc;

use super::queue::SlowVolumes;
use crate::browser::{RenameReport, UndoOffer};

pub(in crate::browser) struct PlannedOp {
    pub plan: OperationPlan,
    pub retain_deletes: Option<RetentionPolicy>,
    pub cannot_retain: bool,
    pub slow_volumes: SlowVolumes,
}

#[derive(Debug, Clone)]
pub struct DropRequest {
    pub kind: OpKind,
    pub sources: Vec<Location>,
    pub target: Location,
    pub confirm: DropConfirm,
    pub source_tab: Option<TabId>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DropConfirm {
    #[default]
    Never,
    Moves,
    Always,
}

pub trait OpLog: JobLog + Send {
    fn check(&mut self, _: LoggedOperation<'_>) -> Result<(), String> {
        Ok(())
    }

    fn execute(&mut self, _: LoggedOperation<'_>) -> Option<JobOutcome> {
        None
    }

    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps) -> Result<(), String>;
}

pub struct LoggedOperation<'a> {
    pub plan: &'a OperationPlan,
    pub ops: &'a dyn ProviderOps,
    pub cancel: &'a CancelToken,
    pub progress: &'a mut dyn FnMut(&Progress),
    pub waiting: &'a mut dyn FnMut(bool),
    pub retain_deletes: Option<RetentionPolicy>,
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
pub type RetainDeletes = Arc<dyn Fn() -> Option<RetentionPolicy> + Send + Sync>;
pub type WriteFactory = Arc<dyn Fn(&Location) -> Result<Box<dyn Send>, FsError> + Send + Sync>;

#[derive(Clone)]
pub struct Operations {
    pub log: LogFactory,
    pub write: Option<WriteFactory>,
    pub retain_deletes: Option<RetainDeletes>,
    pub protected: Vec<Location>,
}

impl Operations {
    #[must_use]
    pub fn unlogged() -> Self {
        Self {
            log: Arc::new(|_| Ok(Box::new(NoLog))),
            write: None,
            retain_deletes: None,
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
    fn finish(&mut self, _: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PendingOp {
    pub id: u64,
    pub sources: Vec<Location>,
    pub target: Option<Location>,
    pub plan: OperationPlan,
    pub retain_deletes: Option<RetentionPolicy>,
    pub cannot_retain: bool,
    pub(crate) source_tab: TabId,
    pub undo: Option<UndoOffer>,
    pub(crate) slow_volumes: Option<SlowVolumes>,
    pub structure: Option<Structure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub base: Location,
    pub keep: bool,
    pub(crate) planning: bool,
    pub(crate) same_volume: bool,
}

impl PendingOp {
    #[must_use]
    pub fn is_planning(&self) -> bool {
        self.structure.as_ref().is_some_and(|s| s.planning)
    }

    #[must_use]
    pub fn is_protected(&self) -> bool {
        self.plan.risks.iter().any(
            |risk| matches!(risk, Risk::Protected(place) if Some(place) != self.target.as_ref()),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Cancelled,
    Failed(String),
}

impl JobStatus {
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
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
    pub is_waiting: bool,
    pub(crate) cancel: CancelToken,
    pub(crate) source_tab: TabId,
    pub undo: bool,
    pub report: RenameReport,
    pub(crate) first_new: Option<String>,
}

impl JobView {
    pub(super) fn plan_failed(request: OpRequest, message: String) -> Self {
        Self {
            id: request.id,
            kind: request.kind,
            from: request.sources.first().and_then(Location::parent),
            target: request.target,
            sources: request.sources.len(),
            progress: Progress {
                bytes_done: 0,
                bytes_total: 0,
                files_done: 0,
                files_total: 0,
                current: None,
                paused: false,
            },
            status: JobStatus::Failed(message),
            is_waiting: false,
            cancel: CancelToken::new(),
            source_tab: request.source_tab,
            undo: false,
            report: RenameReport::default(),
            first_new: None,
        }
    }

    pub(super) fn landing(&self) -> Option<(TabId, Location, String)> {
        let name = self.first_new.clone()?;
        Some((self.source_tab, self.from.clone()?, name))
    }
}

pub(in crate::browser) enum JobUpdate {
    Planned {
        request: OpRequest,
        result: Result<PlannedOp, FsError>,
    },
    Classified {
        id: u64,
        volumes: SlowVolumes,
    },
    Progress {
        id: u64,
        progress: Progress,
    },
    Waiting {
        id: u64,
        is_waiting: bool,
    },
    Done {
        id: u64,
        ended: Ended,
    },
}

pub(in crate::browser) struct Ended {
    pub progress: Option<Progress>,
    pub outcome: Result<JobOutcome, String>,
    pub report: RenameReport,
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
    pub(super) structure: Option<Structure>,
}

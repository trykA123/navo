use navo_core::retention::RetentionPolicy;
use navo_core::{
    ConflictPolicy, FsError, Location, OpKind, OperationPlan, Resolution, Risk, TabId,
};
use navo_fs::{
    CancelToken, CopyMode, DriveClass, HeldFile, JobLog, JobOutcome, Progress, ProviderOps,
    StepOutcome, drive_class,
};

use std::sync::Arc;

use super::clashes::Clash;
use super::queue::SlowVolumes;
use super::stream::{Refused, StreamPaste};
use crate::browser::{RenameReport, UndoOffer};

pub(in crate::browser) struct PlannedOp {
    pub plan: OperationPlan,
    pub retain_deletes: Option<RetentionPolicy>,
    pub cannot_retain: Option<NoKeep>,
    pub slow_volumes: SlowVolumes,
    /// Drive class of the target, probed on the planning thread.
    pub target_class: DriveClass,
    /// Facts of the first conflicts, read on the planning thread.
    pub clashes: Vec<Clash>,
    /// Whether Undo can put back a file this plan overwrites, read on the planning thread.
    pub undoes_overwrites: bool,
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

/// Why a job's log was not made; the job is refused before anything is touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogRefusal {
    /// The drive's retention support could not be determined; the user may run without it.
    RetentionUnknown,
    Failed(String),
}

impl LogRefusal {
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::RetentionUnknown => navo_l10n::tr("job-retention-probe-failed"),
            Self::Failed(message) => message,
        }
    }
}

impl From<String> for LogRefusal {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

pub type LogFactory =
    Arc<dyn Fn(&OperationPlan) -> Result<Box<dyn OpLog>, LogRefusal> + Send + Sync>;
pub type RetainDeletes = Arc<dyn Fn() -> Option<RetentionPolicy> + Send + Sync>;
/// Reads, when a copy or move plan is built, whether its job keeps each overwritten file so
/// Undo can put it back; false when the setting cannot be read.
pub type RetainOverwrites = Arc<dyn Fn() -> bool + Send + Sync>;
pub type WriteFactory = Arc<dyn Fn(&Location) -> Result<Box<dyn Send>, FsError> + Send + Sync>;

#[derive(Clone)]
pub struct Operations {
    pub log: LogFactory,
    /// Builds a job that keeps no old versions, for one job the user agreed to run so.
    pub log_without_retention: Option<LogFactory>,
    pub write: Option<WriteFactory>,
    pub retain_deletes: Option<RetainDeletes>,
    pub protected: Vec<Location>,
    /// Asked for each copy or move plan; `None` (the guard log, retention off) never keeps an
    /// overwritten file, so the dialog warns that overwrites cannot be undone.
    pub retain_overwrites: Option<RetainOverwrites>,
}

/// The retention checks a planning job runs on its own thread.
#[derive(Clone)]
pub(in crate::browser) struct RetentionChecks {
    pub deletes: Option<RetainDeletes>,
    pub overwrites: Option<RetainOverwrites>,
}

impl Operations {
    #[must_use]
    pub fn unlogged() -> Self {
        Self {
            log: Arc::new(|_| Ok(Box::new(NoLog))),
            log_without_retention: None,
            write: None,
            retain_deletes: None,
            protected: Vec::new(),
            retain_overwrites: None,
        }
    }

    /// The checks a planning job runs, cloned to take onto its thread.
    pub(in crate::browser) fn retention_checks(&self) -> RetentionChecks {
        RetentionChecks {
            deletes: self.retain_deletes.clone(),
            overwrites: self.retain_overwrites.clone(),
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
    /// Set when retention is on and the drive the job removes or replaces files on cannot keep them; holds the reason.
    pub cannot_retain: Option<NoKeep>,
    pub keep_old: KeepOld,
    pub(crate) source_tab: TabId,
    pub undo: Option<UndoOffer>,
    pub(crate) slow_volumes: Option<SlowVolumes>,
    pub structure: Option<Structure>,
    /// Set when the files come from another program without paths.
    pub stream: Option<StreamPaste>,
    pub replan: Replan,
    /// Name, size and time of both sides of the first conflicts, for the conflict list.
    pub clashes: Vec<Clash>,
    /// True when the job keeps each file it overwrites, so Undo can put it back; read when the
    /// plan was built (retention on, "Keep overwritten files" on, old versions kept).
    pub undoes_overwrites: bool,
}

/// What a rebuilt plan needs: the conflict policy the user picked, the policy the shown plan was
/// built with (they differ while a rebuild runs), and whether source and target share a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replan {
    pub policy: ConflictPolicy,
    pub(crate) planned: ConflictPolicy,
    pub(crate) same_volume: bool,
}

impl Default for Replan {
    /// A plan that asks (skips names already there) and was built that way.
    fn default() -> Self {
        Self {
            policy: ConflictPolicy::Ask,
            planned: ConflictPolicy::Ask,
            same_volume: false,
        }
    }
}

/// Whether a job keeps old versions as the settings say, or skips them once at the user's word.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum KeepOld {
    #[default]
    AsSet,
    Skip,
}

/// Why a delete keeps no copy although the safety net is on; the confirm dialog says which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoKeep {
    /// The drive cannot keep deleted files, or the check failed.
    Drive,
    /// The drive is below "Stop below free space" (AD14).
    LowSpace,
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
            || self.replan.policy != self.replan.planned
    }

    #[must_use]
    pub fn is_protected(&self) -> bool {
        self.plan.risks.iter().any(
            |risk| matches!(risk, Risk::Protected(place) if Some(place) != self.target.as_ref()),
        )
    }

    /// The chosen items the plan skips as a whole because a name is already there.
    pub(crate) fn skipped_sources(&self) -> usize {
        let skips =
            |c: &&navo_core::Conflict| matches!(c.resolution, Resolution::Skip | Resolution::Ask);
        let conflicts = self.plan.conflicts.iter().filter(skips);
        conflicts
            .filter(|c| self.sources.iter().any(|s| s.same_place(&c.source)))
            .count()
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
    /// Pasted items whose names were refused, so nothing was written for them.
    pub refused: Vec<Refused>,
}

impl JobView {
    pub(super) fn plan_failed(request: OpRequest, message: String) -> Self {
        Self {
            id: request.id,
            kind: request.kind,
            from: request
                .sources
                .first()
                .filter(|_| request.stream.is_none())
                .and_then(Location::parent),
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
            refused: request
                .stream
                .map(|stream| stream.refused)
                .unwrap_or_default(),
        }
    }

    pub(super) fn landing(&self) -> Option<(TabId, Location, String)> {
        let name = self.first_new.clone()?;
        Some((self.source_tab, self.from.clone()?, name))
    }
}

pub(in crate::browser) enum JobUpdate {
    Planned {
        request: Box<OpRequest>,
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
    RetentionUnknown {
        id: u64,
        pending: Box<PendingOp>,
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
    pub(super) stream: Option<StreamPaste>,
    /// How the plan resolves names already at the target; `Ask` (skip them) until the sheet sets it.
    pub(super) policy: ConflictPolicy,
}

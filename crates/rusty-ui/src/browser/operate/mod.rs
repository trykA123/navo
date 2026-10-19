use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{Command, FsError, Location, OpKind, OperationPlan, ProviderPath};
use rusty_fs::{CancelToken, CopyMode, JobOutcome, JobSlot, Priority, Progress, volume_of};

use super::{Browser, Message};

mod paste;
mod types;
mod work;

pub use paste::PasteRefusal;
pub use types::*;
use work::{JobWork, lands_inside, plan_for, start_progress};

impl Browser {
    pub fn set_operations(&mut self, operations: Operations) {
        self.operations = operations;
    }

    pub fn set_copy_mode(&mut self, mode: CopyMode) {
        self.copying.mode = mode;
    }

    #[cfg(test)]
    pub(crate) fn set_class_of(&mut self, class_of: ClassOf) {
        self.copying.class_of = class_of;
    }

    #[must_use]
    pub fn copy_mode(&self) -> CopyMode {
        self.copying.mode
    }

    #[must_use]
    pub fn pending(&self) -> Option<&PendingOp> {
        self.pending.as_ref()
    }

    #[must_use]
    pub fn jobs(&self) -> &[JobView] {
        &self.jobs
    }

    #[must_use]
    pub fn running(&self) -> bool {
        self.jobs.iter().any(|job| job.status == JobStatus::Running)
    }

    pub fn prepare(&mut self, kind: OpKind) -> bool {
        self.op_request(kind)
            .is_some_and(|request| self.submit_plan(request))
    }

    fn submit_plan(&mut self, request: OpRequest) -> bool {
        let Some(volume) = request.sources.first().map(volume_of) else {
            return false;
        };
        self.pending = None;
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume,
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        self.queue.submit(
            slot,
            Box::new(move |cancel| {
                let result = plan_for(provider.as_ref(), &request, cancel);
                let _ = sender.send(Message::Job(JobUpdate::Planned { request, result }));
                wake();
            }),
        );
        true
    }

    pub fn dismiss(&mut self) -> bool {
        self.pending.take().is_some()
    }

    pub fn confirm(&mut self) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        self.jobs
            .retain(|job| matches!(job.status, JobStatus::Running | JobStatus::Failed(_)));
        let cancel = CancelToken::new();
        let mut view = JobView {
            id: pending.id,
            kind: pending.plan.kind,
            from: pending.sources.first().and_then(Location::parent),
            target: pending.target.clone(),
            sources: pending.sources.len(),
            progress: start_progress(&pending.plan),
            status: JobStatus::Running,
            cancel: cancel.clone(),
            source_tab: pending.source_tab,
        };
        if let Err(error) = self.spawn_job(pending, cancel) {
            view.status = JobStatus::Failed(error);
        }
        self.jobs.push(view);
        true
    }

    pub fn shutdown(&mut self, wait: Duration) -> bool {
        for job in &self.jobs {
            if job.status == JobStatus::Running {
                job.cancel.cancel();
            }
        }
        let deadline = Instant::now() + wait;
        while self.running() && Instant::now() < deadline {
            self.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.poll();
        !self.running()
    }

    #[cfg(test)]
    pub(crate) fn jobs_mut(&mut self) -> &mut Vec<JobView> {
        &mut self.jobs
    }

    pub fn take_ended(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.ended)
    }

    pub fn forget_jobs(&mut self, ids: &[u64]) -> bool {
        let before = self.jobs.len();
        self.jobs
            .retain(|job| job.status == JobStatus::Running || !ids.contains(&job.id));
        self.jobs.len() != before
    }

    pub fn dismiss_job(&mut self, id: u64) -> bool {
        self.forget_jobs(&[id])
    }

    pub fn cancel_job(&mut self, id: u64) -> bool {
        let Some(job) = self.jobs.iter().find(|job| job.id == id) else {
            return false;
        };
        job.cancel.cancel();
        true
    }

    pub fn pause_job(&mut self, id: u64) -> bool {
        self.steer_running(id, CancelToken::pause)
    }

    pub fn resume_job(&mut self, id: u64) -> bool {
        self.steer_running(id, CancelToken::resume)
    }

    fn steer_running(&self, id: u64, steer: fn(&CancelToken)) -> bool {
        let running = self
            .jobs
            .iter()
            .find(|job| job.id == id && job.status == JobStatus::Running);
        running.map(|job| steer(&job.cancel)).is_some()
    }

    pub(super) fn accept_job(&mut self, update: JobUpdate) -> bool {
        match update {
            JobUpdate::Planned { request, result } => self.accept_plan(request, result),
            JobUpdate::Progress { id, progress } => {
                self.with_job(id, |job| job.progress = progress)
            }
            JobUpdate::Done {
                id,
                progress,
                outcome,
            } => self.finish_job(id, progress, outcome),
        }
    }

    #[must_use]
    pub fn selected_sources(&self) -> Vec<Location> {
        let tab = self.state.active_tab();
        let Some(listing) = tab.listing() else {
            return Vec::new();
        };
        let marked = tab.selection().marked();
        let rows = if marked.is_empty() {
            vec![tab.view().cursor]
        } else {
            marked
        };
        rows.into_iter()
            .filter_map(|row| listing.get(row))
            .filter_map(|entry| {
                tab.location()
                    .join(&ProviderPath::from_text(&entry.name))
                    .ok()
            })
            .collect()
    }

    fn op_request(&mut self, kind: OpKind) -> Option<OpRequest> {
        let sources = self.selected_sources();
        let tab = self.state.active_tab();
        let target = match kind {
            OpKind::Copy | OpKind::Move => Some(self.other_location()?),
            OpKind::Recycle | OpKind::Delete => None,
        };
        if sources.is_empty()
            || target
                .as_ref()
                .is_some_and(|t| lands_inside(t, tab.location(), &sources))
        {
            return None;
        }
        let same_volume = target
            .as_ref()
            .is_some_and(|t| volume_of(t) == volume_of(tab.location()));
        let source_tab = tab.id();
        self.next_op += 1;
        Some(OpRequest {
            id: self.next_op,
            kind,
            sources,
            target,
            same_volume,
            protected: self.operations.protected.clone(),
            source_tab,
            asks: true,
        })
    }

    fn other_location(&self) -> Option<Location> {
        let state = &self.state;
        let focused = state.focused_pane();
        state
            .window(state.focused_window())?
            .panes()
            .into_iter()
            .find(|pane| pane.id() != focused)
            .map(|pane| pane.active().location().clone())
    }

    fn accept_plan(&mut self, request: OpRequest, result: Result<OperationPlan, FsError>) -> bool {
        match result {
            Ok(plan) => {
                let pending = PendingOp {
                    id: request.id,
                    sources: request.sources,
                    target: request.target,
                    plan,
                    source_tab: request.source_tab,
                };
                let asks = request.asks || pending.is_protected();
                self.pending = Some(pending);
                if !asks {
                    self.confirm();
                }
            }
            Err(error) => self.jobs.push(JobView {
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
                status: JobStatus::Failed(error.message()),
                cancel: CancelToken::new(),
                source_tab: request.source_tab,
            }),
        }
        true
    }

    fn spawn_job(&self, pending: PendingOp, cancel: CancelToken) -> Result<(), String> {
        let work = JobWork {
            id: pending.id,
            copying: self.copying,
            source: pending.sources.first().cloned(),
            target: pending.target,
            plan: pending.plan,
            ops: Arc::clone(&self.provider),
            factory: Arc::clone(&self.operations.log),
            cancel,
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        };
        std::thread::Builder::new()
            .name("ferrite-job".to_owned())
            .spawn(move || work.run())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn with_job(&mut self, id: u64, change: impl FnOnce(&mut JobView)) -> bool {
        let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        change(job);
        true
    }

    fn finish_job(
        &mut self,
        id: u64,
        progress: Option<Progress>,
        outcome: Result<JobOutcome, String>,
    ) -> bool {
        let status = match outcome {
            Ok(JobOutcome::Done) => JobStatus::Done,
            Ok(JobOutcome::Cancelled) => JobStatus::Cancelled,
            Ok(JobOutcome::Failed { error, .. }) => JobStatus::Failed(error.message()),
            Err(message) => JobStatus::Failed(message),
        };
        let mut source_tab = None;
        let is_done = status == JobStatus::Done;
        if matches!(status, JobStatus::Done | JobStatus::Cancelled) {
            self.ended.push(id);
        }
        self.with_job(id, |job| {
            if let Some(progress) = progress {
                job.progress = progress;
            }
            job.status = status;
            source_tab = Some(job.source_tab);
        });
        if is_done && source_tab == Some(self.state.active_tab().id()) {
            let _ = self.command(&Command::SelectionClear);
        }
        self.relist_answering();
        true
    }
}

#[cfg(test)]
mod tests;

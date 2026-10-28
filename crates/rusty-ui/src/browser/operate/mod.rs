use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{Command, FsError, Location, OpKind, Tab};
use rusty_fs::{CancelToken, CopyMode, JobKind, JobOutcome, JobSlot, Priority, volume_of};

use super::{Browser, Message, RenameReport};

mod keep;
mod paste;
pub(in crate::browser) mod queue;
mod types;
mod work;

pub use paste::{PasteRefusal, drop_refusal, refusal_from, same_volume};
pub use queue::SlowVolume;
pub use types::*;
pub(in crate::browser) use work::Run;
use work::{lands_inside, plan_for, start_progress};

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
        self.jobs.iter().any(|job| job.status.is_active())
    }

    pub fn prepare(&mut self, kind: OpKind) -> bool {
        self.op_request(kind)
            .is_some_and(|request| self.submit_plan(request))
    }

    fn submit_plan(&mut self, request: OpRequest) -> bool {
        let Some(volume) = request.sources.first().map(volume_of) else {
            return false;
        };
        if !request.structure.as_ref().is_some_and(|s| s.planning) {
            self.pending = None;
        }
        let retain_deletes = self.operations.retain_deletes.clone();
        let slow = self.slow_volume();
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
        self.pools.queue(JobKind::Listing).submit(
            slot,
            Box::new(move |cancel| {
                let result = plan_for(provider.as_ref(), &request, cancel).map(|plan| {
                    let retained = (plan.kind == OpKind::Delete)
                        .then(|| retain_deletes.as_ref().and_then(|active| active()))
                        .flatten();
                    (
                        plan,
                        retained,
                        queue::slow_volumes(
                            request.sources.iter().chain(request.target.iter()),
                            &slow,
                        ),
                    )
                });
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
        if self.pending.as_ref().is_some_and(PendingOp::is_planning) {
            return false;
        }
        let Some(pending) = self.pending.take() else {
            return false;
        };
        if pending.undo.is_some() {
            return self.start_undo(pending);
        }
        self.start_job(pending, Run::Plain, None);
        true
    }

    pub(in crate::browser) fn start_job(
        &mut self,
        pending: PendingOp,
        run: Run,
        first_new: Option<String>,
    ) {
        self.jobs
            .retain(|job| job.status.is_active() || matches!(job.status, JobStatus::Failed(_)));
        let cancel = CancelToken::new();
        let view = JobView {
            id: pending.id,
            kind: pending.plan.kind,
            from: pending.sources.first().and_then(Location::parent),
            target: pending.target.clone(),
            sources: pending.sources.len(),
            progress: start_progress(&pending.plan),
            status: JobStatus::Queued,
            is_waiting: false,
            cancel: cancel.clone(),
            source_tab: pending.source_tab,
            undo: matches!(run, Run::Undo { .. }),
            report: RenameReport::default(),
            first_new,
        };
        self.jobs.push(view);
        self.enqueue_job(pending, run, cancel);
    }

    pub fn shutdown(&mut self, wait: Duration) -> bool {
        let queued: Vec<_> = self
            .jobs
            .iter()
            .filter(|job| job.status == JobStatus::Queued)
            .map(|job| job.id)
            .collect();
        for id in queued {
            self.cancel_queued(id);
        }
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
            .retain(|job| job.status.is_active() || !ids.contains(&job.id));
        self.jobs.len() != before
    }

    pub fn dismiss_job(&mut self, id: u64) -> bool {
        self.forget_jobs(&[id])
    }

    pub fn cancel_job(&mut self, id: u64) -> bool {
        if self.cancel_queued(id) {
            self.start_queued();
            return true;
        }
        let Some(job) = self.jobs.iter().find(|job| job.id == id && !job.undo) else {
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
            .find(|job| job.id == id && job.status == JobStatus::Running && !job.undo);
        running.map(|job| steer(&job.cancel)).is_some()
    }

    pub(super) fn accept_job(&mut self, update: JobUpdate) -> bool {
        match update {
            JobUpdate::Planned { request, result } => self.accept_plan(request, result),
            JobUpdate::Classified { id, volumes } => self.accept_classified(id, volumes),
            JobUpdate::Progress { id, progress } => {
                self.with_job(id, |job| job.progress = progress)
            }
            JobUpdate::Waiting { id, is_waiting } => {
                self.with_job(id, |job| job.is_waiting = is_waiting)
            }
            JobUpdate::Done { id, ended } => self.finish_job(id, ended),
        }
    }

    #[must_use]
    pub fn selected_sources(&self) -> Vec<Location> {
        selected_sources(self.state.active_tab())
    }

    fn op_request(&mut self, kind: OpKind) -> Option<OpRequest> {
        let sources = self.selected_sources();
        let tab = self.state.active_tab();
        let target = match kind {
            OpKind::Copy | OpKind::Move | OpKind::Link => Some(self.other_location()?),
            OpKind::Recycle | OpKind::Delete => None,
            OpKind::Rename => return None,
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
            .is_some_and(|t| volume_of(t) == volume_of(tab.base_or_location()));
        let structure = keep::structure_for(tab, kind, same_volume);
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
            structure,
        })
    }

    fn other_location(&self) -> Option<Location> {
        self.state
            .pane(self.target_pane()?)
            .map(|pane| pane.active().location().clone())
    }

    fn accept_plan(
        &mut self,
        request: OpRequest,
        result: Result<types::PlannedOp, FsError>,
    ) -> bool {
        if self.is_stale_replan(&request) {
            return false;
        }
        match result {
            Ok((plan, retain_deletes, slow_volumes)) => {
                let pending = PendingOp {
                    id: request.id,
                    sources: request.sources,
                    target: request.target,
                    plan,
                    retain_deletes,
                    source_tab: request.source_tab,
                    undo: None,
                    slow_volumes: Some(slow_volumes),
                    structure: request.structure.map(|s| Structure {
                        planning: false,
                        ..s
                    }),
                };
                let asks = request.asks || pending.is_protected();
                self.pending = Some(pending);
                if !asks {
                    self.confirm();
                }
            }
            Err(error) => {
                if request.structure.as_ref().is_some_and(|s| s.planning) {
                    self.pending = None;
                }
                self.jobs
                    .push(JobView::plan_failed(request, error.message()));
            }
        }
        true
    }

    fn with_job(&mut self, id: u64, change: impl FnOnce(&mut JobView)) -> bool {
        let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        change(job);
        true
    }

    fn finish_job(&mut self, id: u64, ended: Ended) -> bool {
        let Ended {
            progress,
            outcome,
            report,
        } = ended;
        self.note_rename_error(id, &outcome);
        let status = match outcome {
            Ok(JobOutcome::Done) => JobStatus::Done,
            Ok(JobOutcome::Cancelled) => JobStatus::Cancelled,
            Ok(JobOutcome::Failed { error, .. }) => JobStatus::Failed(error.message()),
            Err(message) => JobStatus::Failed(message),
        };
        let mut source_tab = None;
        let mut landing = None;
        let mut undo = false;
        let is_done = status == JobStatus::Done;
        if matches!(status, JobStatus::Done | JobStatus::Cancelled) {
            self.ended.push(id);
        }
        self.with_job(id, |job| {
            if let Some(progress) = progress {
                job.progress = progress;
            }
            job.status = status;
            job.is_waiting = false;
            job.report = report;
            source_tab = Some(job.source_tab);
            landing = job.landing();
            undo = job.undo;
        });
        if is_done && let Some((tab, folder, name)) = landing {
            self.land_on(tab, &folder, name);
        }
        if is_done && !undo && source_tab == Some(self.state.active_tab().id()) {
            let _ = self.command(&Command::SelectionClear);
        }
        self.relist_answering();
        self.release_job(id);
        true
    }
}

fn selected_sources(tab: &Tab) -> Vec<Location> {
    keep::selected_names(tab)
        .into_iter()
        .filter_map(|name| tab.entry_location(name).ok())
        .collect()
}

#[cfg(test)]
mod tests;

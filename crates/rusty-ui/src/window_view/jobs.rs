use std::time::{Duration, Instant};

use rusty_core::Location;

use super::FerriteWindow;
use super::operate::JOB_LINGER;
use super::strip::StripUi;

use crate::activity::Activity;
use crate::browser::{JobStatus, JobView};
use crate::job_card::{JobCard, JobInfo, JobsSummary, Pause, RenameNote, SpeedMeter};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::tokens;

const SAMPLE_EVERY: Duration = Duration::from_millis(500);

pub(super) struct JobUi {
    pub(super) is_card_hidden: bool,
    pub(super) meter: SpeedMeter,
    pub(super) activity: Activity,
    pub(super) strip: StripUi,
    pub(super) perf: super::perf_recording::PerfUi,
    is_sampling: bool,
}

impl JobUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<FerriteWindow>) -> Self {
        Self {
            is_card_hidden: true,
            meter: SpeedMeter::default(),
            activity: Activity::default(),
            strip: StripUi::new(window, cx),
            perf: super::perf_recording::PerfUi::default(),
            is_sampling: false,
        }
    }
}

fn name_of(location: &Location) -> Option<SharedString> {
    location
        .name()
        .map(|name| name.display().into_owned().into())
}

pub(super) fn job_info(job: &JobView, speed: Option<u64>) -> JobInfo {
    let progress = &job.progress;
    JobInfo {
        id: job.id,
        kind: job.kind,
        items: job.sources,
        from: job.from.as_ref().and_then(name_of),
        target: job
            .target
            .as_ref()
            .map(|target| target.as_str().to_owned().into()),
        current: progress.current.as_ref().and_then(name_of),
        bytes_done: progress.bytes_done,
        bytes_total: progress.bytes_total,
        files_done: progress.files_done,
        files_total: progress.files_total,
        state: match &job.status {
            JobStatus::Running => JobRowState::Running,
            JobStatus::Done => JobRowState::Done,
            JobStatus::Cancelled => JobRowState::Cancelled,
            JobStatus::Failed(message) => JobRowState::Failed(message.clone().into()),
        },
        speed,
        pause: match (job.cancel.is_paused(), progress.paused) {
            (false, _) => Pause::Off,
            (true, false) => Pause::Asked,
            (true, true) => Pause::On,
        },
        rename: RenameNote {
            undo: job.undo,
            skipped: job.report.skipped,
            left: job.report.left.iter().filter_map(name_of).collect(),
            log_error: job.report.log_error.clone().map(Into::into),
        },
    }
}

impl FerriteWindow {
    pub(super) fn jobs_woke(&mut self, cx: &mut Context<Self>) {
        self.sample_job_speeds(cx.background_executor().now());
        self.refresh_activity(cx);
        if self.job_ui.is_sampling || !self.browser.running() {
            return;
        }
        self.job_ui.is_sampling = true;
        cx.spawn(async move |this, cx| {
            let mut is_running = true;
            while is_running {
                cx.background_executor().timer(SAMPLE_EVERY).await;
                is_running = this
                    .update(cx, |view, cx| view.sample_while_running(cx))
                    .unwrap_or(false);
            }
        })
        .detach();
    }

    fn sample_while_running(&mut self, cx: &mut Context<Self>) -> bool {
        self.sample_job_speeds(cx.background_executor().now());
        self.refresh_activity(cx);
        self.job_ui.is_sampling = self.browser.running();
        self.job_ui.is_sampling
    }

    fn sample_job_speeds(&mut self, now: Instant) {
        let meter = &mut self.job_ui.meter;
        let jobs = self.browser.jobs();
        for job in jobs.iter().filter(|job| job.status == JobStatus::Running) {
            let bytes = job.progress.bytes_done;
            if job.progress.paused {
                meter.hold(job.id, bytes, now);
            } else {
                meter.observe(job.id, bytes, now);
            }
        }
        meter.retain(&jobs.iter().map(|job| job.id).collect::<Vec<_>>());
    }

    pub(super) fn sync_activity(&mut self, now: Instant) {
        let jobs = self.job_infos();
        self.job_ui.activity.sync(&jobs, now);
    }

    pub(super) fn jobs_changed(&mut self) {
        if self.browser.jobs().is_empty() {
            self.job_ui.is_card_hidden = true;
        }
    }

    pub(super) fn jobs_key(&mut self, cx: &mut Context<Self>) {
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.app_menu.is_some()
            || self.is_multi_rename_open();
        if is_busy || self.browser.jobs().is_empty() {
            cx.propagate();
            return;
        }
        self.toggle_jobs(cx);
    }

    fn toggle_jobs(&mut self, cx: &mut Context<Self>) {
        if !self.browser.jobs().is_empty() {
            self.job_ui.is_card_hidden = !self.job_ui.is_card_hidden;
            cx.notify();
        }
    }

    pub(super) fn show_job_card(&mut self, cx: &mut Context<Self>) {
        self.job_ui.is_card_hidden = false;
        cx.notify();
    }

    pub(super) fn job_infos(&self) -> Vec<JobInfo> {
        let meter = &self.job_ui.meter;
        self.browser
            .jobs()
            .iter()
            .map(|job| job_info(job, meter.speed(job.id)))
            .collect()
    }

    pub(super) fn job_card_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let jobs = self.job_infos();
        if jobs.is_empty() || self.job_ui.is_card_hidden {
            return None;
        }
        let t = tokens::current(cx).clone();
        let is_ended = JobsSummary::of(&jobs).is_ended();
        let (shrink, pause, resume) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        let (cancel, dismiss) = (cx.weak_entity(), cx.weak_entity());
        let card = JobCard::new(jobs)
            .when(is_ended, |card| card.fade_out(JOB_LINGER))
            .on_shrink(move |_, cx| {
                let _ = shrink.update(cx, |view, cx| view.toggle_jobs(cx));
            })
            .on_pause(move |id, _, cx| {
                let _ = pause.update(cx, |view, cx| view.pause_job(id, cx));
            })
            .on_resume(move |id, _, cx| {
                let _ = resume.update(cx, |view, cx| view.resume_job(id, cx));
            })
            .on_cancel(move |id, _, cx| {
                let _ = cancel.update(cx, |view, cx| view.cancel_job(id, cx));
            })
            .on_dismiss(move |id, _, cx| {
                let _ = dismiss.update(cx, |view, cx| view.dismiss_job(id, cx));
            });
        Some(
            div()
                .absolute()
                .right(t.spacing.xxl)
                .bottom(t.spacing.md)
                .child(card)
                .into_any_element(),
        )
    }
}

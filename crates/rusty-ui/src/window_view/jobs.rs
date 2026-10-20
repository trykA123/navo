use std::time::{Duration, Instant};

use rusty_core::Location;

use super::FerriteWindow;
use super::operate::JOB_LINGER;
use gpui_kit::base::TestSupportExt;

use crate::activity::Activity;
use crate::browser::{JobStatus, JobView};
use crate::fkey_bar::FKeyBar;
use crate::job_card::{JobCard, JobInfo, JobPill, JobsSummary, Pause, PillFit, SpeedMeter};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::tokens;

const SAMPLE_EVERY: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct JobUi {
    pub(super) is_pill: bool,
    pub(super) meter: SpeedMeter,
    pub(super) activity: Activity,
    is_sampling: bool,
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
    }
}

impl FerriteWindow {
    pub(super) fn jobs_woke(&mut self, cx: &mut Context<Self>) {
        self.sample_job_speeds(cx.background_executor().now());
        self.sync_activity(cx.background_executor().now());
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
        self.sync_activity(cx.background_executor().now());
        cx.notify();
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

    fn sync_activity(&mut self, now: Instant) {
        let jobs = self.job_infos();
        self.job_ui.activity.sync(&jobs, now);
    }

    pub(super) fn jobs_changed(&mut self) {
        if self.browser.jobs().is_empty() {
            self.job_ui.is_pill = false;
        }
    }

    pub(super) fn jobs_key(&mut self, cx: &mut Context<Self>) {
        let is_busy =
            self.prompt.is_some() || self.browser.pending().is_some() || self.app_menu.is_some();
        if is_busy || self.browser.jobs().is_empty() {
            cx.propagate();
            return;
        }
        self.toggle_jobs(cx);
    }

    fn toggle_jobs(&mut self, cx: &mut Context<Self>) {
        if !self.browser.jobs().is_empty() {
            self.job_ui.is_pill = !self.job_ui.is_pill;
            cx.notify();
        }
    }

    pub(super) fn job_infos(&self) -> Vec<JobInfo> {
        let meter = &self.job_ui.meter;
        self.browser
            .jobs()
            .iter()
            .map(|job| job_info(job, meter.speed(job.id)))
            .collect()
    }

    fn job_pill(&self, jobs: &[JobInfo], fit: PillFit, cx: &mut Context<Self>) -> JobPill {
        let this = cx.weak_entity();
        JobPill::new(jobs, fit)
            .when(JobsSummary::of(jobs).is_ended(), |pill| {
                pill.fade_out(JOB_LINGER)
            })
            .on_click(move |_, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_jobs(cx));
            })
    }

    pub(super) fn with_job_pill(
        &self,
        bar: FKeyBar,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> FKeyBar {
        let jobs = self.job_infos();
        if jobs.is_empty() {
            return bar;
        }
        let fit = PillFit::for_width(window.viewport_size().width);
        let bar = bar.reserve_pill_width(fit.slot_width());
        if !self.job_ui.is_pill {
            return bar;
        }
        bar.right_slot(self.job_pill(&jobs, fit, cx))
    }

    pub(super) fn pill_strip(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = tokens::current(cx).spacing.fkey_height;
        let fit = PillFit::for_width(window.viewport_size().width);
        div()
            .id("job-strip")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .h(height)
            .px(tokens::current(cx).spacing.md)
            .child(self.job_pill(&self.job_infos(), fit, cx))
    }

    pub(super) fn job_card_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let jobs = self.job_infos();
        if jobs.is_empty() || self.job_ui.is_pill {
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

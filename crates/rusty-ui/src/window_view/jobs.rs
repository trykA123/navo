use std::time::Instant;

use rusty_core::Location;

use super::FerriteWindow;
use super::operate::JOB_LINGER;
use crate::browser::{JobStatus, JobView};
use crate::fkey_bar::FKeyBar;
use crate::job_card::{JobCard, JobInfo, JobPill, JobsSummary, PillFit, SpeedMeter};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::tokens;

#[derive(Default)]
pub(super) struct JobUi {
    pub(super) is_pill: bool,
    pub(super) meter: SpeedMeter,
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
    }
}

impl FerriteWindow {
    pub(super) fn sample_job_speeds(&mut self) {
        let now = Instant::now();
        let meter = &mut self.job_ui.meter;
        let jobs = self.browser.jobs();
        for job in jobs.iter().filter(|job| job.status == JobStatus::Running) {
            meter.observe(job.id, job.progress.bytes_done, now);
        }
        meter.retain(&jobs.iter().map(|job| job.id).collect::<Vec<_>>());
    }

    pub(super) fn jobs_changed(&mut self) {
        if self.browser.jobs().is_empty() {
            self.job_ui.is_pill = false;
        }
    }

    pub(super) fn toggle_jobs(&mut self, cx: &mut Context<Self>) {
        if !self.browser.jobs().is_empty() {
            self.job_ui.is_pill = !self.job_ui.is_pill;
            cx.notify();
        }
    }

    fn job_infos(&self) -> Vec<JobInfo> {
        let meter = &self.job_ui.meter;
        self.browser
            .jobs()
            .iter()
            .map(|job| job_info(job, meter.speed(job.id)))
            .collect()
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
        let this = cx.weak_entity();
        let pill = JobPill::new(&jobs, fit)
            .when(JobsSummary::of(&jobs).is_ended(), |pill| {
                pill.fade_out(JOB_LINGER)
            })
            .on_click(move |_, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_jobs(cx));
            });
        bar.right_slot(pill)
    }

    pub(super) fn job_card_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let jobs = self.job_infos();
        if jobs.is_empty() || self.job_ui.is_pill {
            return None;
        }
        let t = tokens::current(cx).clone();
        let is_ended = JobsSummary::of(&jobs).is_ended();
        let (shrink, cancel, dismiss) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        let card = JobCard::new(jobs)
            .when(is_ended, |card| card.fade_out(JOB_LINGER))
            .on_shrink(move |_, cx| {
                let _ = shrink.update(cx, |view, cx| view.toggle_jobs(cx));
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
                .bottom(t.spacing.fkey_height + t.spacing.md)
                .child(card)
                .into_any_element(),
        )
    }
}

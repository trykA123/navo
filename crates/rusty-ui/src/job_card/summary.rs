use std::time::Duration;

use gpui_kit::SharedString;
use rusty_core::OpKind;

use rusty_l10n::tr;

use crate::format;
use crate::job_row::JobRowState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pause {
    Off,
    Asked,
    On,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobInfo {
    pub id: u64,
    pub kind: OpKind,
    pub items: usize,
    pub from: Option<SharedString>,
    pub target: Option<SharedString>,
    pub current: Option<SharedString>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub state: JobRowState,
    pub speed: Option<u64>,
    pub pause: Pause,
}

fn verbs(kind: OpKind) -> [String; 3] {
    match kind {
        OpKind::Copy => ["Copy", "Copying", "Copied"],
        OpKind::Move => ["Move", "Moving", "Moved"],
        OpKind::Recycle => ["Recycle", "Recycling", "Recycled"],
        OpKind::Delete => ["Delete", "Deleting", "Deleted"],
        OpKind::Rename => ["Rename", "Renaming", "Renamed"],
        OpKind::Link => return ["link-verb", "link-verb-running", "link-verb-done"].map(tr),
    }
    .map(str::to_owned)
}

fn fraction_of(done: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.;
    }
    (done as f64 / total as f64).clamp(0., 1.) as f32
}

fn percent_of(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.).floor())
}

pub fn time_text(left: Duration) -> String {
    let seconds = left.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn speed_text(speed: u64) -> String {
    format!("{}/s", format::size(speed))
}

impl JobInfo {
    fn items_text(&self) -> String {
        format::count(
            u64::try_from(self.items).unwrap_or(u64::MAX),
            "item",
            "items",
        )
    }

    pub fn name(&self) -> String {
        format!("{} {}", verbs(self.kind)[0].as_str(), self.items_text())
    }

    pub fn heading(&self) -> String {
        let [name, running, done] = verbs(self.kind);
        let verb = match self.state {
            JobRowState::Running => running,
            JobRowState::Done => done,
            JobRowState::Failed(_) | JobRowState::Cancelled => name,
        };
        format!("{verb} {}", self.items_text())
    }

    pub fn route(&self) -> Option<String> {
        let from = self.from.as_deref();
        match (self.kind, from, self.target.as_deref()) {
            (OpKind::Recycle, Some(from), _) => Some(format!("{from} → Recycle Bin")),
            (OpKind::Delete, Some(from), _) => Some(format!("in {from}")),
            (_, Some(from), Some(to)) => Some(format!("{from} → {to}")),
            (_, None, Some(to)) => Some(format!("to {to}")),
            _ => None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.state == JobRowState::Running
    }

    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            return fraction_of(self.files_done, self.files_total);
        }
        fraction_of(self.bytes_done, self.bytes_total)
    }

    pub fn percent_text(&self) -> String {
        percent_of(self.fraction())
    }

    pub fn bytes_text(&self) -> String {
        format!(
            "{} / {}",
            format::size(self.bytes_done),
            format::size(self.bytes_total)
        )
    }

    pub fn files_text(&self) -> String {
        format!(
            "{} of {}",
            format::bytes(self.files_done),
            format::count(self.files_total, "file", "files")
        )
    }

    pub fn is_paused(&self) -> bool {
        self.is_running() && self.pause == Pause::On
    }

    fn live_speed(&self) -> Option<u64> {
        self.speed.filter(|_| self.pause == Pause::Off)
    }

    pub fn left(&self) -> Option<Duration> {
        let speed = self.live_speed().filter(|speed| *speed > 0)?;
        let remaining = self.bytes_total.saturating_sub(self.bytes_done);
        Some(Duration::from_secs(remaining.div_ceil(speed)))
    }

    pub fn status_text(&self) -> String {
        match &self.state {
            JobRowState::Running => {}
            JobRowState::Done => return "Done".to_owned(),
            JobRowState::Cancelled => return "Cancelled".to_owned(),
            JobRowState::Failed(_) => return "Failed".to_owned(),
        }
        let mut parts = vec![self.percent_text(), self.files_text()];
        match self.pause {
            Pause::Off => {
                parts.extend(self.speed.map(speed_text));
                parts.extend(self.left().map(|left| format!("{} left", time_text(left))));
            }
            Pause::Asked => parts.push("Pausing…".to_owned()),
            Pause::On => parts.push("Paused".to_owned()),
        }
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobsSummary {
    pub count: usize,
    pub name: String,
    pub fraction: f32,
    pub left: Option<Duration>,
    pub speed: Option<u64>,
    pub has_failed: bool,
    pub is_running: bool,
    pub is_paused: bool,
    pub is_cancelled: bool,
}

impl JobsSummary {
    pub fn of(jobs: &[JobInfo]) -> Self {
        let bytes = jobs.iter().fold((0, 0), |(done, total), job| {
            (done + job.bytes_done, total + job.bytes_total)
        });
        let files = jobs.iter().fold((0, 0), |(done, total), job| {
            (done + job.files_done, total + job.files_total)
        });
        let (done, total) = if bytes.1 == 0 { files } else { bytes };
        let running = || jobs.iter().filter(|job| job.is_running());
        let speeds: Vec<u64> = running().filter_map(JobInfo::live_speed).collect();
        let name = match jobs {
            [only] => only.name(),
            _ => format!("{} jobs", jobs.len()),
        };
        Self {
            count: jobs.len(),
            name,
            fraction: match jobs {
                [only] => only.fraction(),
                _ => fraction_of(done, total),
            },
            left: running().filter_map(JobInfo::left).max(),
            speed: (!speeds.is_empty()).then(|| speeds.iter().sum()),
            has_failed: jobs
                .iter()
                .any(|job| matches!(job.state, JobRowState::Failed(_))),
            is_running: running().next().is_some(),
            is_paused: running().next().is_some() && running().all(JobInfo::is_paused),
            is_cancelled: jobs.iter().all(|job| job.state == JobRowState::Cancelled),
        }
    }

    pub fn percent_text(&self) -> String {
        percent_of(self.fraction)
    }

    pub fn is_ended(&self) -> bool {
        self.count > 0 && !self.is_running && !self.has_failed
    }

    pub fn word(&self) -> Option<&'static str> {
        match (self.is_running, self.has_failed, self.is_cancelled) {
            (true, ..) => None,
            (false, true, _) => Some("Failed"),
            (false, false, true) => Some("Cancelled"),
            (false, false, false) => Some("Done"),
        }
    }
}

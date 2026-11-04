use serde::Serialize;

use crate::measure::{Metric, Stat};
use crate::privacy::{Report, SafeText};
use crate::timings::{Subject, TimingsReport};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimingFolder {
    Chosen,
    Generated100k,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimingStep {
    List,
    Sort,
    FilterKey,
    WindowFirstUsable,
    KeyToFrame,
}

/// One folder timing. `index` tells repeated steps apart (sort key, keystroke); never text.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TimingRow {
    pub folder: TimingFolder,
    pub step: TimingStep,
    pub index: u8,
    pub samples: u32,
    pub median_micros: u64,
    pub p95_micros: u64,
}

/// The folder timings of the headless pipeline and the window; filled by the folder phase.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Timings {
    NotMeasured { reason: SafeText },
    Measured { rows: Vec<TimingRow> },
}

impl Timings {
    pub fn rows(&self) -> &[TimingRow] {
        match self {
            Self::Measured { rows } => rows,
            Self::NotMeasured { .. } => &[],
        }
    }

    pub(super) fn missing_reason(&self) -> SafeText {
        match self {
            Self::NotMeasured { reason } => *reason,
            Self::Measured { .. } => crate::safe_text!("no timing for this step"),
        }
    }
}

fn row(
    folder: TimingFolder,
    step: TimingStep,
    index: u8,
    metric: &Metric<Stat>,
) -> Option<TimingRow> {
    match metric {
        Metric::Measured { value } => Some(TimingRow {
            folder,
            step,
            index,
            samples: value.samples,
            median_micros: value.median_micros,
            p95_micros: value.p95_micros,
        }),
        Metric::NotMeasured { .. } => None,
    }
}

impl Timings {
    /// Collects the rows of the folder phase; no timing report means not measured.
    pub fn from_reports(reports: &[Report]) -> Self {
        let mut rows = Vec::new();
        let mut seen = false;
        for report in reports {
            if let Report::Timings(timings) = report {
                seen = true;
                push_rows(&mut rows, timings);
            }
        }
        if seen {
            Self::Measured { rows }
        } else {
            Self::NotMeasured {
                reason: crate::safe_text!("folder timings did not run"),
            }
        }
    }
}

fn push_rows(rows: &mut Vec<TimingRow>, timings: &TimingsReport) {
    match timings {
        TimingsReport::Generated(_) => {}
        TimingsReport::Headless(headless) => {
            let folder = folder_of(headless.subject);
            rows.extend(row(folder, TimingStep::List, 0, &headless.list));
            for (index, sort) in (0u8..).zip(&headless.sorts) {
                rows.extend(row(folder, TimingStep::Sort, index, &sort.time));
            }
            let filter = &headless.filter_keystroke;
            rows.extend(row(folder, TimingStep::FilterKey, 0, filter));
        }
        TimingsReport::FirstUsable(usable) => {
            let folder = folder_of(usable.subject);
            let step = TimingStep::WindowFirstUsable;
            rows.extend(row(folder, step, 0, &usable.first_usable));
        }
        TimingsReport::KeyToFrame(keys) => {
            let folder = folder_of(keys.subject);
            rows.extend(row(folder, TimingStep::KeyToFrame, 0, &keys.key_to_frame));
        }
    }
}

fn folder_of(subject: Subject) -> TimingFolder {
    match subject {
        Subject::Owner => TimingFolder::Chosen,
        Subject::Generated => TimingFolder::Generated100k,
    }
}

#[cfg(test)]
mod from_reports_tests {
    use super::*;
    use crate::timings::HeadlessReport;

    #[test]
    fn no_timing_report_is_not_measured() {
        assert!(matches!(
            Timings::from_reports(&[]),
            Timings::NotMeasured { .. }
        ));
    }

    #[test]
    fn unavailable_numbers_give_no_rows() {
        let report = Report::Timings(TimingsReport::Headless(HeadlessReport::unavailable(
            Subject::Owner,
            crate::safe_text!("no exe"),
        )));
        let timings = Timings::from_reports(&[report]);
        assert!(matches!(timings, Timings::Measured { .. }));
        assert!(timings.rows().is_empty());
    }
}

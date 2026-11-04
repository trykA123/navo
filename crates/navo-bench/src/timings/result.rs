use serde::Serialize;

use super::host::Subject;
use crate::measure::{Metric, Stat};
use crate::privacy::SafeText;

/// The four sort keys of the headless benchmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortKey {
    Size,
    Date,
    Extension,
    Name,
}

impl SortKey {
    pub const ALL: [Self; 4] = [Self::Size, Self::Date, Self::Extension, Self::Name];

    /// The label `navo bench --folder` uses for this key.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Date => "date",
            Self::Extension => "extension",
            Self::Name => "name",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SortStat {
    pub key: SortKey,
    pub time: Metric<Stat>,
}

/// The generated folder: file count and the time the guarded build took.
#[derive(Clone, Debug, Serialize)]
pub struct GeneratedReport {
    pub files: u32,
    pub build_micros: Metric<u64>,
}

/// `navo bench --folder`: open+list, each sort, and the filter keystrokes, in microseconds.
#[derive(Clone, Debug, Serialize)]
pub struct HeadlessReport {
    pub subject: Subject,
    /// Spawns that ran; `failed` of them gave no numbers.
    pub attempted: u32,
    pub failed: u32,
    pub entries: Metric<u64>,
    pub top_level_bytes: Metric<u64>,
    pub list: Metric<Stat>,
    pub sorts: Vec<SortStat>,
    pub filter_keystroke: Metric<Stat>,
}

/// First usable time with the folder in the left pane, from `--metrics`.
#[derive(Clone, Debug, Serialize)]
pub struct UsableReport {
    pub subject: Subject,
    pub attempted: u32,
    pub failed: u32,
    pub first_usable: Metric<Stat>,
}

/// Key-to-frame of plain letters posted to Navo's window, from `--trace`.
#[derive(Clone, Debug, Serialize)]
pub struct KeysReport {
    pub subject: Subject,
    pub attempted: u32,
    pub failed: u32,
    pub keys_posted: u32,
    /// Trace records read back; equal to `keys_posted` when every posted key was traced.
    pub records: u32,
    pub key_to_frame: Metric<Stat>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "measure", rename_all = "kebab-case")]
pub enum TimingsReport {
    Generated(GeneratedReport),
    Headless(HeadlessReport),
    FirstUsable(UsableReport),
    KeyToFrame(KeysReport),
}

impl HeadlessReport {
    /// Nothing could run; every number is missing for one reason.
    pub fn unavailable(subject: Subject, reason: SafeText) -> Self {
        Self {
            subject,
            attempted: 0,
            failed: 0,
            entries: Metric::missing(reason),
            top_level_bytes: Metric::missing(reason),
            list: Metric::missing(reason),
            sorts: SortKey::ALL
                .map(|key| SortStat {
                    key,
                    time: Metric::missing(reason),
                })
                .to_vec(),
            filter_keystroke: Metric::missing(reason),
        }
    }
}

impl UsableReport {
    pub fn unavailable(subject: Subject, reason: SafeText) -> Self {
        Self {
            subject,
            attempted: 0,
            failed: 0,
            first_usable: Metric::missing(reason),
        }
    }
}

impl KeysReport {
    pub fn unavailable(subject: Subject, reason: SafeText) -> Self {
        Self {
            subject,
            attempted: 0,
            failed: 0,
            keys_posted: 0,
            records: 0,
            key_to_frame: Metric::missing(reason),
        }
    }
}

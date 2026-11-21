//! Presentation model for the large compare screen: per-stage meters, the D5 tiles,
//! the running-read line and the slow-file note. It reads engine updates and holds no GPUI state.

use sidewise_l10n::t;
use sidewise_ui::browser::{ComparePhase, CompareTotals, CompareUpdate, SlowPath};

/// A running read shows in the Large view once it is this old (ms).
const READING_SHOWN_MS: u64 = 1000;

/// The number of walk stages the meter strip shows, one per `ComparePhase`.
const PHASES: usize = 3;

/// One stage's progress: items done and the total when known.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Meter {
    pub done: u64,
    pub total: Option<u64>,
}

/// The three stage meters of the newest generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhaseMeters {
    generation: u64,
    meters: [Meter; PHASES],
}

/// One D5 tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub label: &'static str,
    pub value: u64,
    pub note: &'static str,
}

impl Meter {
    /// The fraction done in `0.0..=1.0`; `None` while the total is unknown, `1.0` for an empty one.
    pub fn fraction(self) -> Option<f32> {
        let total = self.total?;
        if total == 0 {
            return Some(1.0);
        }
        Some(self.done.min(total) as f32 / total as f32)
    }
}

impl PhaseMeters {
    /// Applies one engine update; a newer generation resets the meters before progress writes.
    /// Returns true on a changed meter or reset, and false for stale or failed updates.
    pub fn observe(&mut self, update: &CompareUpdate) -> bool {
        let generation = match update {
            CompareUpdate::Rows { generation, .. }
            | CompareUpdate::Totals { generation, .. }
            | CompareUpdate::Finished { generation }
            | CompareUpdate::Stopped { generation, .. }
            | CompareUpdate::Continued { generation }
            | CompareUpdate::PhaseProgress { generation, .. }
            | CompareUpdate::SlowPath { generation, .. }
            | CompareUpdate::Reading { generation, .. } => *generation,
            CompareUpdate::Failed(_) => return false,
        };
        if generation < self.generation {
            return false;
        }
        let reset = generation > self.generation;
        if reset {
            self.generation = generation;
            self.meters = [Meter::default(); PHASES];
        }
        let CompareUpdate::PhaseProgress {
            phase, done, total, ..
        } = update
        else {
            return reset;
        };
        let meter = Meter {
            done: *done,
            total: *total,
        };
        let slot = &mut self.meters[phase_slot(*phase)];
        let changed = *slot != meter;
        *slot = meter;
        changed || reset
    }

    /// The meter of one walk stage; a stage that never reported reads zero.
    pub fn meter(&self, phase: ComparePhase) -> Meter {
        self.meters[phase_slot(phase)]
    }
}

/// The array slot of a walk stage: listing, quick check, content check.
fn phase_slot(phase: ComparePhase) -> usize {
    match phase {
        ComparePhase::Listing => 0,
        ComparePhase::QuickCheck => 1,
        ComparePhase::ContentCheck => 2,
    }
}

/// The four D5 tiles, in display order: differences, only right, same, checking.
pub fn tiles(totals: &CompareTotals) -> [Tile; 4] {
    [
        Tile {
            label: "Differences",
            value: totals.different + totals.only_left + totals.only_right + totals.conflicts,
            note: "found so far",
        },
        Tile {
            label: "Only right",
            value: totals.only_right,
            note: "only on the right",
        },
        Tile {
            label: "Same",
            value: totals.same + totals.same_size_time,
            note: "by size and time",
        },
        Tile {
            label: "Checking",
            value: totals.checking,
            note: "content check queue",
        },
    ]
}

/// The slow-file note; `None` when no file has been slow yet.
pub fn slow_note(slow: &[SlowPath]) -> Option<String> {
    let slowest = slow.iter().map(|path| path.elapsed_ms).max()?;
    let seconds = slowest as f64 / 1000.0;
    Some(match slow.len() {
        1 => format!("1 slow file so far; it took {seconds:.1} s."),
        count => format!("{count} slow files so far; the slowest took {seconds:.1} s."),
    })
}

/// The running-read line, e.g. `Reading a/b.bin · 6.2 s`; `None` when no read runs or it
/// is younger than 1 s, so quick files never flicker past.
pub fn reading_line(reading: Option<&SlowPath>) -> Option<String> {
    let reading = reading.filter(|read| read.elapsed_ms >= READING_SHOWN_MS)?;
    let seconds = format!("{:.1}", reading.elapsed_ms as f64 / 1000.0);
    Some(t!(
        "compare-large-reading",
        path = reading.path.as_str(),
        seconds = seconds
    ))
}

/// The heading and sub-line of one walk stage.
pub fn phase_label(phase: ComparePhase) -> (&'static str, &'static str) {
    match phase {
        ComparePhase::Listing => ("Listing", "Both trees, breadth-first"),
        ComparePhase::QuickCheck => ("Size and time", "The quick check, no reads"),
        ComparePhase::ContentCheck => ("Content check", "Visible rows first"),
    }
}

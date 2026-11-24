//! Test-only counters for the app soak: how many per-tab entries, pending loads and jobs the
//! browser holds, so a long key loop can check that nothing grows round after round.

use super::Browser;

impl Browser {
    /// Named counts of the browser's per-tab maps and queues; each should return to its
    /// baseline once the soak loop's tabs and viewers are closed again.
    pub(crate) fn soak_counts(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("size_owners", self.size_owners.len()),
            ("finds", self.finds.len()),
            ("details", self.details.len()),
            ("errors", self.errors.len()),
            ("late_errors", self.late_errors.len()),
            ("focus_after_load", self.focus_after_load.len()),
            ("filters", self.filters.len()),
            ("hidden_marks", self.hidden_marks.len()),
            ("full", self.full.len()),
            ("sorts", self.sorts.len()),
            ("shown", self.shown.len()),
            ("cached", self.cached.len()),
            ("sorted", self.sorted.len()),
            ("in_flight", self.in_flight),
            ("jobs", self.jobs.len()),
            ("ended", self.ended.len()),
            ("seen", self.seen.len()),
            ("pins", self.pins.len()),
        ]
    }
}

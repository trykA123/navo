//! The large text body of the file diff (V38, D16): rows from the hunk list and the line windows
//! the view asks for. Arriving hunks never move the top line; next difference past the known
//! hunks waits with "Looking further… N %" until one arrives, and Esc stops the wait.

mod lines;
mod list;
mod map;
#[cfg(test)]
mod probes;

use std::cell::RefCell;
use std::rc::Rc;

use navo_l10n::{t, tr};

#[cfg(test)]
pub(in crate::window_view) use map::Anchor;
pub(in crate::window_view) use map::RowMap;

use crate::compare_diff::{
    DiffUpdate, LargeHunk, LargeProgress, LargeStage, LargeStepView, Layout,
};
use crate::prelude::*;
use crate::primitives::ListScroll;
use lines::LineCache;

/// The hunks, steps and progress of the applied large text snapshot, and how the body shows them.
pub(super) struct LargeShown {
    pub(super) hunks: Vec<LargeHunk>,
    pub(super) steps: Vec<LargeStepView>,
    pub(super) progress: Option<LargeProgress>,
    map: Rc<RowMap>,
    lines: Rc<RefCell<LineCache>>,
    scroll: ListScroll,
    /// The row height of the last frame; zero before the first one.
    row_height: Pixels,
    /// The current change, by its first lines, so it survives hunks arriving above it.
    current: Option<(u64, u64)>,
    /// Whether next difference waits for a hunk past the known ones.
    seeking: bool,
}

impl Default for LargeShown {
    fn default() -> Self {
        Self {
            hunks: Vec::new(),
            steps: Vec::new(),
            progress: None,
            map: Rc::new(RowMap::new(&[], Layout::SideBySide, None)),
            lines: Rc::default(),
            scroll: ListScroll::new(),
            row_height: px(0.),
            current: None,
            seeking: false,
        }
    }
}

impl LargeShown {
    /// Applies a snapshot. The top line stays on top; a waiting next difference jumps to the
    /// first new hunk past the current one, or stops once the diff finished without one.
    pub(super) fn snapshot(
        &mut self,
        hunks: Vec<LargeHunk>,
        steps: Vec<LargeStepView>,
        progress: Option<LargeProgress>,
    ) {
        let map = RowMap::new(&hunks, self.map.layout(), self.map.totals());
        (self.hunks, self.steps, self.progress) = (hunks, steps, progress);
        self.remap(map);
        if !self.seeking {
            return;
        }
        if let Some(next) = self.after_current() {
            self.seeking = false;
            self.go_to(next);
        } else if self.finished() {
            self.seeking = false;
        }
    }

    /// Stores a line window that starts at line `start` with both sides' line counts.
    pub(super) fn add_lines(
        &mut self,
        start: u64,
        (left, right): (Vec<String>, Vec<String>),
        totals: (u64, u64),
    ) {
        self.lines.borrow_mut().add(start, left, right);
        if self.map.totals() != Some(totals) {
            self.remap(RowMap::new(&self.hunks, self.map.layout(), Some(totals)));
        }
    }

    /// Replaces the row map, keeping the top line on top (D16).
    fn remap(&mut self, map: RowMap) {
        let top = self.top();
        let anchor = self.map.anchor(top);
        self.map = Rc::new(map);
        if let Some(anchor) = anchor {
            let row = self.map.row_of(anchor);
            if row != top {
                self.scroll
                    .scroll_to_top(usize::try_from(row).unwrap_or(usize::MAX));
            }
        }
    }

    /// The top row of the list in the last frame.
    fn top(&self) -> u64 {
        u64::try_from(self.scroll.top_row(self.row_height)).unwrap_or(u64::MAX)
    }

    /// Whether the diff reported its last section.
    fn finished(&self) -> bool {
        self.progress.is_some_and(|progress| {
            progress.stage == LargeStage::Sections
                && progress.total > 0
                && progress.done >= progress.total
        })
    }

    /// The index of the current change in the sorted hunks.
    fn current_index(&self) -> Option<usize> {
        let (left, right) = self.current?;
        self.map
            .hunks()
            .iter()
            .position(|hunk| hunk.left_rows.start == left && hunk.right_rows.start == right)
    }

    /// The first hunk past the current change, or with none, the first at or below the top row.
    fn after_current(&self) -> Option<usize> {
        let hunks = self.map.hunks();
        match self.current {
            Some((left, right)) => hunks
                .iter()
                .position(|hunk| (hunk.left_rows.start, hunk.right_rows.start) > (left, right)),
            None => {
                let top = self.top();
                (0..hunks.len()).find(|&n| self.map.hunk_row(n).is_some_and(|row| row >= top))
            }
        }
    }

    /// Makes hunk `n` current and centres it.
    fn go_to(&mut self, n: usize) {
        let Some(hunk) = self.map.hunks().get(n) else {
            return;
        };
        self.current = Some((hunk.left_rows.start, hunk.right_rows.start));
        if let Some(row) = self.map.hunk_row(n) {
            self.scroll
                .scroll_to_center(usize::try_from(row).unwrap_or(usize::MAX));
        }
    }

    /// Next (`forward`) or previous change. Forward past the known hunks starts the wait while
    /// the diff runs. False when nothing changed.
    pub(super) fn step(&mut self, forward: bool) -> bool {
        if forward {
            if let Some(next) = self.after_current() {
                self.go_to(next);
                return true;
            }
            let wait = !self.finished() && !self.seeking;
            self.seeking |= wait;
            return wait;
        }
        self.seeking = false;
        let previous = match self.current_index() {
            Some(at) => at.checked_sub(1),
            None => {
                let top = self.top();
                let hunks = self.map.hunks().len();
                (0..hunks)
                    .rev()
                    .find(|&n| self.map.hunk_row(n).is_some_and(|row| row < top))
            }
        };
        previous.is_some_and(|n| {
            self.go_to(n);
            true
        })
    }

    /// Esc: stops a waiting next difference; false when none was waiting.
    pub(super) fn stop_wait(&mut self) -> bool {
        std::mem::take(&mut self.seeking)
    }

    /// The status line: "Looking further… N %" while waiting, else the change counter.
    pub(super) fn status(&self) -> SharedString {
        if self.seeking {
            let percent = self.progress.map_or(0, |progress| {
                progress.done.saturating_mul(100) / progress.total.max(1)
            });
            return t!("compare-screen-looking-further", percent = percent.min(100)).into();
        }
        let total = self.map.hunks().len();
        let text = match (self.current_index(), total) {
            (_, 0) => tr("compare-screen-no-changes"),
            (Some(at), _) => t!("compare-screen-change", current = at + 1, total = total),
            (None, _) => t!("compare-screen-changes", total = total),
        };
        text.into()
    }
}

impl super::FileScreen {
    /// Applies a large snapshot or line window. A snapshot is acknowledged only after it is
    /// applied, so the engine frees only what this view already dropped.
    pub(super) fn apply_large(&mut self, update: DiffUpdate) {
        let large = self.large.get_or_insert_with(LargeShown::default);
        match update {
            DiffUpdate::Large {
                hunks,
                steps,
                progress,
                ack,
                ..
            } => {
                large.snapshot(hunks, steps, progress);
                self.control.acknowledge(ack);
            }
            DiffUpdate::LargeLines {
                rows,
                left,
                right,
                totals,
                ..
            } => large.add_lines(rows.start, (left, right), totals),
            _ => {}
        }
    }

    /// The status line's change counter: the text pair's, else the large body's.
    pub(super) fn changes(&self) -> Option<SharedString> {
        let text = self.view.as_ref().map(super::file::FileView::status);
        text.or_else(|| self.large.as_ref().map(LargeShown::status))
    }
}

impl crate::window_view::NavoWindow {
    /// Esc that stays on the file diff: stops a waiting next difference, else drops the
    /// picked lines. False when Esc should go back.
    pub(super) fn esc_in_place(&mut self) -> bool {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let large = file.and_then(|file| file.large.as_mut());
        large.is_some_and(LargeShown::stop_wait) || self.clear_picks()
    }
}

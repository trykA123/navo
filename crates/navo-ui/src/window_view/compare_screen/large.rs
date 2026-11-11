//! The large text body of the file diff (V38, D16): rows from the hunk list and the line windows
//! the view asks for. Arriving hunks never move the top line; next difference past the known
//! hunks waits with "Looking further… N %" until one arrives, and Esc stops the wait.

mod bar;
mod here;
mod lines;
mod list;
mod map;
mod notice;
#[cfg(test)]
mod probes;
mod scroll;

use std::cell::RefCell;
use std::rc::Rc;

use navo_l10n::{t, tr};

pub(in crate::window_view) use map::{Anchor, RowMap, Settled};

use crate::compare_diff::{
    DiffUpdate, LargeHunk, LargeProgress, LargeStage, LargeStep, LargeStepView, Layout, is_long,
    line_cols,
};
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::window_view::compare_screen::sideways::SidewaysHandle;
use bar::FileBar;
use lines::LineCache;

/// The hunks, steps and progress of the applied large text snapshot, and how the body shows them.
pub(super) struct LargeShown {
    pub(super) hunks: Vec<LargeHunk>,
    pub(super) steps: Vec<LargeStepView>,
    pub(super) progress: Option<LargeProgress>,
    map: Rc<RowMap>,
    lines: Rc<RefCell<LineCache>>,
    scroll: ListScroll,
    /// The list window's first row (D16: small `f32` offsets at 10M lines), the row height and
    /// the scrollbar over the whole file.
    bar: FileBar,
    /// The current change, by its first lines, so it survives hunks arriving above it.
    current: Option<(u64, u64)>,
    /// Whether next difference waits for a hunk past the known ones.
    seeking: bool,
    /// The line to put on top once the line counts are known (Show text from bytes, V41d).
    sought: Option<Anchor>,
    /// The sideways scroll both text columns share (W8j). The large body never wraps, so this
    /// handle keeps wrap off; a new pair starts at offset 0.
    sideways: SidewaysHandle,
}

impl LargeShown {
    /// An empty body in `layout` with rows `row_height` tall.
    pub(super) fn new(row_height: Pixels, layout: Layout) -> Self {
        let scroll = ListScroll::new();
        Self {
            hunks: Vec::new(),
            steps: Vec::new(),
            progress: None,
            map: Rc::new(RowMap::new(&[], layout, None, Settled::default())),
            lines: Rc::default(),
            bar: FileBar::new(&scroll, row_height),
            scroll,
            current: None,
            seeking: false,
            sought: None,
            sideways: SidewaysHandle::default(),
        }
    }

    /// The sideways state, for the wheel listener and the Left/Right keys beside the file.
    pub(super) fn sideways(&self) -> SidewaysHandle {
        self.sideways.clone()
    }

    /// Applies a snapshot. The top line stays on top; a waiting next difference jumps to the
    /// first new hunk past the current one, or stops once the diff finished without one.
    pub(super) fn snapshot(
        &mut self,
        hunks: Vec<LargeHunk>,
        steps: Vec<LargeStepView>,
        progress: Option<LargeProgress>,
    ) {
        (self.hunks, self.steps, self.progress) = (hunks, steps, progress);
        self.remap(self.rebuild(self.map.layout(), self.map.totals()));
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
    pub(super) fn add_lines(&mut self, fetched: lines::Fetched, totals: (u64, u64)) {
        self.widen_sideways(&fetched);
        let near = self.near(fetched.start);
        self.lines.borrow_mut().add(fetched, near);
        if self.map.totals() != Some(totals) {
            self.remap(self.rebuild(self.map.layout(), Some(totals)));
        }
    }

    /// Raises the sideways extent to the widest line of the window `fetched` (W8j), as the file
    /// diff's `LineCols` counts it: a line over `LONG_LINE` is skipped, and the extent only
    /// grows — a narrower window arriving later never shrinks it.
    fn widen_sideways(&self, fetched: &lines::Fetched) {
        let wide = (fetched.lines.0.iter().chain(fetched.lines.1.iter()))
            .filter(|line| !is_long(line))
            .map(|line| line_cols(line))
            .max()
            .unwrap_or(0);
        if wide > self.sideways.widest().unwrap_or(0) {
            self.sideways.set_widest(wide);
        }
    }

    /// Shows the rows in `layout`, keeping the top line on top.
    pub(super) fn relayout(&mut self, layout: Layout) {
        if self.map.layout() != layout {
            self.remap(self.rebuild(layout, self.map.totals()));
        }
    }

    /// A row map of the applied hunks in `layout` with line counts `totals`.
    fn rebuild(&self, layout: Layout, totals: Option<(u64, u64)>) -> RowMap {
        RowMap::new(&self.hunks, layout, totals, self.settled())
    }

    /// What the diff settled: done once the last section reported, and the evicted sections
    /// that no hunk came back to (a re-diffed section has its hunks again).
    fn settled(&self) -> Settled {
        let mut starts: Vec<u64> = self.hunks.iter().map(|hunk| hunk.left_rows.start).collect();
        starts.sort_unstable();
        let evicted = (self.steps.iter())
            .filter(|step| step.step == LargeStep::PagedHunks)
            .map(|step| step.section.clone())
            .filter(|section| {
                let at = starts.partition_point(|&start| start < section.start);
                starts.get(at).is_none_or(|&start| start >= section.end)
            })
            .collect();
        Settled {
            done: self.finished(),
            evicted,
        }
    }

    /// Replaces the row map, keeping the top line on top (D16); reads the position now, so
    /// back-to-back remaps before a frame keep the same line.
    fn remap(&mut self, map: RowMap) {
        let (top, sub) = (self.top(), self.sub());
        let anchor = self.map.anchor(top);
        self.map = Rc::new(map);
        self.bar.set_count(self.map.count());
        let row = anchor.map_or(top, |anchor| self.map.row_of(anchor));
        self.place(row, sub);
        self.apply_sought();
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
            self.place(row.saturating_sub(self.visible() / 2), 0.);
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
    /// Applies a large snapshot or line window with rows `row_height` tall, then asks for the
    /// lines in view. A snapshot is acknowledged only after it is applied, so the engine frees
    /// only what this view already dropped.
    pub(super) fn apply_large(&mut self, update: DiffUpdate, row_height: Pixels) {
        let layout = self.look.layout.unwrap_or(self.auto);
        let large = (self.large).get_or_insert_with(|| LargeShown::new(row_height, layout));
        large.bar.set_row_height(row_height);
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
                left_starts,
                right_starts,
                totals,
                ..
            } => large.add_lines(
                lines::Fetched {
                    start: rows.start,
                    lines: (left, right),
                    starts: (left_starts, right_starts),
                },
                totals,
            ),
            _ => {}
        }
        large.ask(self.control.as_ref());
    }

    /// The status line's change counter: the text pair's, else the large body's, else the
    /// hex regions, else the image pair's similarity.
    pub(super) fn changes(&self) -> Option<SharedString> {
        let text = self.view.as_ref().map(super::file::FileView::status);
        let large = self.large.as_ref().map(|large| match large.notes() {
            Some(notes) => format!("{} · {notes}", large.status()).into(),
            None => large.status(),
        });
        text.or(large)
            .or_else(|| self.hex.as_ref().and_then(super::hex::HexView::status))
            .or_else(|| {
                self.pictures
                    .as_ref()
                    .and_then(super::image::Pictures::status)
            })
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

//! Test probes of the large text body: its row count, top row and status, one row's lines, and
//! a scroll to a row.

use crate::compare_diff::ShownKind;
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// What a test reads of the large body.
pub(in crate::window_view) struct LargeProbe {
    pub(in crate::window_view) count: u64,
    pub(in crate::window_view) top: u64,
    pub(in crate::window_view) status: SharedString,
}

/// One side's line number and text, or none.
type Line = Option<(u64, String)>;

impl NavoWindow {
    /// The open large body's row count, top row and status line.
    pub(in crate::window_view) fn large_probe(&self) -> Option<LargeProbe> {
        let large = self
            .compare
            .screen
            .as_ref()?
            .file
            .as_ref()?
            .large
            .as_ref()?;
        Some(LargeProbe {
            count: large.map.count(),
            top: large.top(),
            status: large.status(),
        })
    }

    /// The kind of list row `row` and the line number and text on each side.
    pub(in crate::window_view) fn large_row(&self, row: u64) -> Option<(ShownKind, Line, Line)> {
        let large = self
            .compare
            .screen
            .as_ref()?
            .file
            .as_ref()?
            .large
            .as_ref()?;
        let shown = large.map.at(row);
        let lines = large.lines.borrow();
        let side = |left: bool, line: Option<u64>| {
            line.map(|at| (at, lines.line(left, at).unwrap_or_default().to_owned()))
        };
        Some((shown.kind, side(true, shown.left), side(false, shown.right)))
    }

    /// Scrolls the large body so row `row` is on top.
    pub(in crate::window_view) fn large_scroll_to(&mut self, row: usize) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(large) = file.and_then(|file| file.large.as_mut()) {
            large.scroll.scroll_to_top(row);
        }
    }
}

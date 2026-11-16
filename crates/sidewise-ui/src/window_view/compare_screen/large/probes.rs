//! Test probes of the large text body: its row count, top row and status, one row's lines, a
//! scroll to a row, and its sideways state.

use crate::compare_diff::ShownKind;
use crate::prelude::*;
use crate::window_view::ScreenOwner;
use crate::window_view::compare_screen::SidewaysSeen;

/// What a test reads of the large body.
#[doc(hidden)]
pub struct LargeProbe {
    #[doc(hidden)]
    pub count: u64,
    #[doc(hidden)]
    pub top: u64,
    /// The first row of the list's window.
    #[doc(hidden)]
    pub base: u64,
    #[doc(hidden)]
    pub status: SharedString,
    /// The progress line and step notices beside the status, joined; empty when none.
    #[doc(hidden)]
    pub notes: SharedString,
    /// Whether the link to the memory budget setting is shown.
    #[doc(hidden)]
    pub budget_link: bool,
}

/// One side's line number and text, or none.
type Line = Option<(u64, String)>;

impl ScreenOwner {
    /// The open large body's row count, top row and status line.
    #[doc(hidden)]
    pub fn large_probe(&self) -> Option<LargeProbe> {
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
            base: large.bar.base(),
            status: large.status(),
            notes: large.notes().unwrap_or_default(),
            budget_link: large.budget_link(),
        })
    }

    /// The kind of list row `row` and the line number and text on each side.
    #[doc(hidden)]
    pub fn large_row(&self, row: u64) -> Option<(ShownKind, Line, Line)> {
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

    /// Scrolls the large body so row `row` is on top, as a scroll event would.
    #[doc(hidden)]
    pub fn large_scroll_to(&mut self, row: usize) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(file) = file
            && let Some(large) = file.large.as_mut()
        {
            large.place(u64::try_from(row).unwrap_or(u64::MAX), 0.);
            large.ask(file.control.as_ref());
        }
    }

    /// The sideways offset, this frame's cut and the extent of the large body the screen draws:
    /// the open one, else the one a bytes switch holds (as in `host.rs`).
    #[doc(hidden)]
    pub fn large_sideways(&self) -> Option<SidewaysSeen> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        let large = file.large.as_ref().or(file.switch.held_large())?;
        Some(large.sideways.seen())
    }
}

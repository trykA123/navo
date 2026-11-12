//! The two panes a folder compare borrows: what each showed before, and which had focus.
//!
//! The session model keeps no pane state; `CompareSlot` holds this binding beside it, so a
//! compare can be driven with no panes in it at all.

use navo_core::{Location, PaneId};

/// A pane as it was before the compare started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Saved {
    pub pane: PaneId,
    pub location: Location,
    pub cursor: Option<String>,
}

/// The panes of one compare: both sides as they were, and the pane to give focus back to.
pub(super) struct PaneBinding {
    pub(super) saved: [Saved; 2],
    pub(super) focused: PaneId,
}

impl PaneBinding {
    /// 0 for the left saved pane, 1 for the right one; `None` for a pane outside the compare.
    #[must_use]
    pub(super) fn side_of(&self, pane: PaneId) -> Option<usize> {
        self.saved.iter().position(|saved| saved.pane == pane)
    }
}

//! Folder compare as a mode of the two panes (D10): state, events and paired navigation.
//!
//! The app starts the engine through `CompareHooks`; updates arrive on the browser's channel
//! tagged with the run that asked for them, so a left compare never feeds a new one.

mod hooks;
mod mode;
#[doc(hidden)]
pub mod rows;
mod stream;
mod view;

pub use hooks::{
    CompareControl, CompareHooks, CompareKind, ComparePhase, CompareRow, CompareSide, CompareSink,
    CompareTotals, CompareUpdate, RowProgress, RowStatus, SlowPath, StopBound,
};
pub use mode::{CompareFilter, CompareMode, ExitStep, FileDiffRequest, FolderSession};
pub use view::{BarView, FILTERS, RowView, SideView, Tone, Total, bar_view, empty_text, pane_rows};
#[doc(hidden)]
pub use view::{look_of, status_text};

/// Why a compare did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareRefusal {
    NoOtherPane,
    NotAvailable,
}

impl CompareRefusal {
    #[must_use]
    pub fn fluent_id(self) -> &'static str {
        match self {
            Self::NoOtherPane => "compare-needs-two-panes",
            Self::NotAvailable => "compare-could-not-start",
        }
    }
}

#[cfg(test)]
mod phase_export_tests {
    use crate::browser::ComparePhase;

    #[test]
    fn every_compare_phase_is_exported_in_walk_order() {
        assert_eq!(
            ComparePhase::ALL,
            [
                ComparePhase::Listing,
                ComparePhase::QuickCheck,
                ComparePhase::ContentCheck,
            ]
        );
    }
}

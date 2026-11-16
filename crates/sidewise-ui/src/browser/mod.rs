//! The browser-facing side of the compare: the folder-compare model and its update contract.

use std::sync::Arc;

pub mod compare;

pub use compare::{
    BarView, CompareControl, CompareFilter, CompareHooks, CompareKind, CompareMode, ComparePhase,
    CompareRefusal, CompareRow, CompareSide, CompareSink, CompareTotals, CompareUpdate, ExitStep,
    FILTERS, FileDiffRequest, FolderSession, RowProgress, RowStatus, RowView, SideView, SlowPath,
    StopBound, Tone, Total, bar_view, empty_text, pane_rows,
};
#[doc(hidden)]
pub use compare::{look_of, status_text};

/// Wakes the window after a background compare update.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

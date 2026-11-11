//! The full-window compare screens: file diff and Stream. This module holds the UI contract.
//! The app implements `DiffHooks`; `navo-ui` never depends on the compare engine.

pub mod body;
pub mod card;
pub mod copy_bar;
#[cfg(test)]
pub(crate) mod fake;
pub mod hooks;
pub mod image_body;
pub mod map;
pub mod rows;
pub mod sheet;
pub mod spans;
pub mod stream_model;
pub mod ticks;

pub use card::{CardCtx, stream_row_view};
pub use hooks::{
    ByteProgress, ByteRegionView, Card, CardRow, CopyChange, CopyDirection, CopyDone, CopyResult,
    CopySink, DiffHooks, DiffRow, DiffSink, DiffUpdate, HunkView, ImageAlpha, ImageLimit,
    ImageMask, ImageScores, ImageSize, LargeHunk, LargeProgress, LargeStage, LargeStep,
    LargeStepReason, LargeStepView, Layout, PairControl, PairFacts, PairKind, PairRequest,
    PairText, RowSpans, StreamControl, StreamSink, StreamUpdate, ViewedKey,
};
pub use rows::{Half, ShownKind, ShownRow, StreamRow, pair_rows, row_count, stream_rows};
pub use sheet::{SheetPlan, SheetPlanSink, SheetResult, SheetRow, SheetSink};
pub use spans::SpanCache;
pub use stream_model::{
    EscStep, Stop, StreamItem, StreamList, StreamModel, is_stream_item, stream_items,
};
pub use ticks::{OnTick, PickSide, Ticks, line_on};

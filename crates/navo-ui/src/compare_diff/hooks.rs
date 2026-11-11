//! Plain UI types and traits the app implements for the file diff and the Stream.
//! All loading, diffing, hashing and searching run on app threads (D24).

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use navo_core::Location;

use super::sheet::{SheetPlan, SheetPlanSink, SheetResult, SheetSink};
use crate::settings::Values;

mod large;

pub use large::{
    ByteProgress, ByteRegionView, HexAlign, LargeHunk, LargeProgress, LargeStage, LargeStep,
    LargeStepReason, LargeStepView,
};

/// One file pair to open. `id` is the compare row id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairRequest {
    pub id: u64,
    pub path: String,
    pub left: Option<Location>,
    pub right: Option<Location>,
}

/// How a diff is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Unified,
    SideBySide,
}

/// A hash of both files' content; keys the Viewed mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewedKey(pub [u8; 64]);

/// What a pair holds. `Unavailable` carries a Fluent id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    Text,
    Large { left: u64, right: u64 },
    Binary,
    Image,
    OneSided { left: bool },
    Unavailable(&'static str),
}

/// One aligned row; the numbers are line indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRow {
    Equal(u32, u32),
    Changed(u32, u32),
    Deleted(u32),
    Inserted(u32),
}

/// One hunk. It is copyable when `digest` is `Some`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkView {
    pub rows: Range<u32>,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
    pub digest: Option<[u8; 16]>,
    pub added: u32,
    pub removed: u32,
}

/// Encodings, line endings and fallbacks, as Fluent ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairFacts {
    pub left_encoding: &'static str,
    pub right_encoding: &'static str,
    pub left_endings: &'static str,
    pub right_endings: &'static str,
    pub raw_equal: bool,
    pub display_equal: bool,
    pub fallbacks: Vec<&'static str>,
}

/// The whole text of a pair, owned and shared by `Arc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairText {
    pub left: Vec<String>,
    pub right: Vec<String>,
    /// The byte offset of each left line in its file, plus the end offset; empty when unknown.
    pub left_starts: Vec<u64>,
    /// The same for the right lines; empty when unknown.
    pub right_starts: Vec<u64>,
    pub rows: Vec<DiffRow>,
    pub hunks: Vec<HunkView>,
    pub facts: PairFacts,
    pub viewed: ViewedKey,
}

/// One row of a Stream card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardRow {
    Line {
        row: DiffRow,
        hunk: u32,
        left: Option<String>,
        right: Option<String>,
    },
    Gap {
        lines: u32,
    },
    More {
        hunks: u32,
    },
}

/// A Stream card: the changes of one file with context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub kind: PairKind,
    pub rows: Vec<CardRow>,
    pub hunks: Vec<HunkView>,
    pub added: u32,
    pub removed: u32,
    pub viewed_key: ViewedKey,
    pub viewed: bool,
    /// False when a side could not be hashed; V does nothing so keys never collide on zeros.
    pub viewable: bool,
}

/// Changed spans inside a row, as byte offsets into the row's `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpans {
    pub row: u32,
    pub left: Vec<Range<u32>>,
    pub right: Vec<Range<u32>>,
}

/// Pixel dimensions of one image side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

/// How far two images' alpha channels differ; `differs` is true above zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageAlpha {
    pub max_delta: u8,
    pub mean_delta: f64,
    pub differs: bool,
}

/// Why an image pair was not compared in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLimit {
    /// The encoded file is over the 64 MiB read cap.
    Encoded,
    /// A side is over the 64 MP cap, or a WebP is over its 48 MP cap.
    Pixels,
    /// A side does not fit the decoder's allocation limits.
    Decode,
}

/// An image pair's result: full scores, or the cheaper fallbacks of D20.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageScores {
    /// Structural, RGB and histogram scores; 1.0 is identical.
    Scored {
        ssim: f64,
        rgb_min_rms_similarity: f64,
        luma_hellinger_similarity: f64,
        alpha: Option<ImageAlpha>,
    },
    /// A side is over 64 MP, so only the raw bytes were compared.
    HashOnly { equal: bool },
    /// Different pixel dimensions prevent scoring.
    SizeMismatch { left: ImageSize, right: ImageSize },
    /// The pair cannot be read or decoded within the image limits; `equal` is the byte hash.
    TooLarge { reason: ImageLimit, equal: bool },
    /// The file names a format the image decoder does not support, such as SVG.
    Unsupported { format: &'static str },
}

/// A grayscale difference mask: 0 means equal and 255 the largest difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMask {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// What an open file diff reports. A stale `request` is dropped on arrival.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffUpdate {
    Opened {
        request: u64,
        kind: PairKind,
        text: Option<Arc<PairText>>,
    },
    Spans {
        request: u64,
        spans: Vec<RowSpans>,
    },
    Large {
        request: u64,
        hunks: Vec<LargeHunk>,
        steps: Vec<LargeStepView>,
        progress: Option<LargeProgress>,
        /// This snapshot's number; the view acknowledges it after applying the snapshot.
        ack: u64,
    },
    /// The display lines of a large text pair, read off the UI thread. A side past its end
    /// has fewer lines than `rows`; a `*_cut` side's last line is longer than the read limit
    /// and ends early. `totals` holds both sides' line counts. `*_starts` hold the byte offset
    /// of each line in `left` and `right`, in the same order.
    LargeLines {
        request: u64,
        rows: Range<u64>,
        left: Vec<String>,
        right: Vec<String>,
        left_starts: Vec<u64>,
        right_starts: Vec<u64>,
        left_cut: bool,
        right_cut: bool,
        totals: (u64, u64),
    },
    Bytes {
        request: u64,
        align: HexAlign,
        /// The run `byte_regions` issued; regions from an older run are stale.
        run: u64,
        regions: Vec<ByteRegionView>,
        progress: Option<ByteProgress>,
    },
    Image {
        request: u64,
        scores: ImageScores,
        /// Present only for a scored pair.
        mask: Option<ImageMask>,
    },
    ChangedOnDisk {
        request: u64,
    },
    Failed {
        request: u64,
        text: String,
    },
}

/// What a running Stream reports. A stale `generation` is dropped on arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamUpdate {
    Card {
        generation: u64,
        id: u64,
        card: Arc<Card>,
    },
    Evicted {
        generation: u64,
        id: u64,
    },
    Viewed {
        id: u64,
        viewed: bool,
    },
    Progress {
        generation: u64,
        built: u32,
        total: u32,
    },
    /// The files that matched since the last `Hits` of this query, not all so far.
    Hits {
        generation: u64,
        query: u64,
        hits: Vec<(u64, Vec<u32>)>,
        searched: u32,
        total: u32,
        done: bool,
    },
}

/// Which way a change is copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDirection {
    ToLeft,
    ToRight,
}

/// One change to copy: the pair, the shown byte ranges and digest, and the direction (D29).
/// `lines` holds the source-side line numbers to copy (V32); `None` copies the whole change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyChange {
    pub pair: PairRequest,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
    pub digest: [u8; 16],
    pub direction: CopyDirection,
    pub lines: Option<BTreeSet<usize>>,
}

/// How a copy ended. A refusal carries the text for the status line; nothing was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    Copied,
    Refused(String),
}

/// The end of one copy, for the pair whose `request` id was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyDone {
    pub request: u64,
    pub direction: CopyDirection,
    pub result: CopyResult,
}

/// Receives the end of a copy on any thread; it wakes the window.
pub type CopySink = Arc<dyn Fn(CopyDone) + Send + Sync>;

/// Receives file diff updates on any thread; it wakes the window.
pub type DiffSink = Arc<dyn Fn(DiffUpdate) + Send + Sync>;

/// Receives Stream updates on any thread; it wakes the window.
pub type StreamSink = Arc<dyn Fn(StreamUpdate) + Send + Sync>;

/// An open file diff. No call blocks; dropping it cancels without blocking.
pub trait PairControl: Send {
    /// Asks for the changed spans of the visible `rows`.
    fn spans(&self, rows: Range<u32>);
    /// Reloads both sides under `request`, an id above every earlier one; updates then carry it.
    fn refresh(&self, request: u64);
    /// Moves a large diff's focus to `line`; a text pair ignores it.
    fn focus(&self, _line: u64) {}
    /// Asks for the display lines of a large text pair; a text pair ignores it.
    fn lines(&self, _rows: Range<u64>) {}
    /// Releases the eviction charges of snapshots up to number `upto` once the view applied it.
    fn acknowledge(&self, _upto: u64) {}
    /// Diffs the two sides as bytes with `align`; a text pair ignores it.
    fn byte_regions(&self, _align: HexAlign, _run: u64) {}
    /// Reloads both sides under `request` like `refresh`, and keeps opening the pair as bytes
    /// (`bytes`) or as its own kind (not `bytes`) on every later reload. A forced view only
    /// changes a text or large pair; the default ignores the choice.
    fn view_as(&self, request: u64, _bytes: bool) {
        self.refresh(request);
    }
}

/// A running Stream. No call blocks; dropping it cancels without blocking.
pub trait StreamControl: Send {
    /// Replaces the item list; the `bool` says the item is settled.
    fn set_items(&self, generation: u64, items: Vec<(PairRequest, bool)>);
    /// Builds the cards of `ids` first.
    fn focus(&self, ids: Vec<u64>);
    /// Stores the Viewed mark of an item under its content key.
    fn set_viewed(&self, id: u64, key: ViewedKey, viewed: bool);
    /// Searches every difference; empty `text` stops the search.
    fn search(&self, query: u64, text: String);
    /// Drops the card of item `id` and builds it again from the files as they are now, under
    /// the same generation; the other cards stay.
    fn reload(&self, id: u64);
}

/// Opens file diffs and Streams; the app implements it over the compare engine.
pub trait DiffHooks: Send + Sync {
    /// Opens one pair using the compare settings in `values`.
    fn open_pair(
        &self,
        request: PairRequest,
        values: &Values,
        sink: DiffSink,
    ) -> Box<dyn PairControl>;

    /// Opens a Stream using the compare settings in `values`.
    fn open_stream(&self, values: &Values, sink: StreamSink) -> Box<dyn StreamControl>;

    /// Copies one change and reports once through `done`, never blocking the caller. The
    /// default refuses, for a backend that cannot write.
    fn copy_change(&self, change: CopyChange, done: CopySink) {
        done(CopyDone {
            request: change.pair.id,
            direction: change.direction,
            result: CopyResult::Refused(navo_l10n::tr("compare-copy-not-copyable")),
        });
    }

    /// Plans a copy of many changes without writing and reports once through `planned`. The
    /// app keeps the plan for `run_sheet`. The default plans no file.
    fn plan_sheet(&self, _changes: Vec<CopyChange>, planned: SheetPlanSink) {
        planned(SheetPlan::default());
    }

    /// Runs the last plan as one job and reports once through `done`, never blocking the
    /// caller. The default refuses, for a backend that cannot write.
    fn run_sheet(&self, done: SheetSink) {
        done(SheetResult::Refused(navo_l10n::tr(
            "compare-copy-not-copyable",
        )));
    }

    /// Copies several changes of one pair as one job (one undo step) and reports once through
    /// `done`. The default sends a lone change to `copy_change` and refuses more.
    fn copy_changes(&self, mut changes: Vec<CopyChange>, done: CopySink) {
        match (changes.pop(), changes.is_empty()) {
            (Some(change), true) => self.copy_change(change, done),
            (first, _) => done(CopyDone {
                request: first.as_ref().map_or(0, |change| change.pair.id),
                direction: first.map_or(CopyDirection::ToRight, |change| change.direction),
                result: CopyResult::Refused(navo_l10n::tr("compare-copy-not-copyable")),
            }),
        }
    }
}

#[cfg(test)]
mod tests;

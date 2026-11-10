//! Plain UI types and traits the app implements for the file diff and the Stream.
//! All loading, diffing, hashing and searching run on app threads (D24).

use std::ops::Range;
use std::sync::Arc;

use navo_core::Location;

use crate::settings::Values;

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

/// What an open file diff reports. A stale `request` is dropped on arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    Hits {
        generation: u64,
        query: u64,
        hits: Vec<(u64, Vec<u32>)>,
        searched: u32,
        total: u32,
        done: bool,
    },
}

/// Receives file diff updates on any thread; it wakes the window.
pub type DiffSink = Arc<dyn Fn(DiffUpdate) + Send + Sync>;

/// Receives Stream updates on any thread; it wakes the window.
pub type StreamSink = Arc<dyn Fn(StreamUpdate) + Send + Sync>;

/// An open file diff. No call blocks; dropping it cancels without blocking.
pub trait PairControl: Send {
    /// Asks for the changed spans of the visible `rows`.
    fn spans(&self, rows: Range<u32>);
    /// Reloads both sides.
    fn refresh(&self);
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
}

#[cfg(test)]
mod tests;

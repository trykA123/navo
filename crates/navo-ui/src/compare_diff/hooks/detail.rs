//! What a Stream card of a large, binary or image pair shows past its summary (V42, D26.1-D26.5).

use std::ops::Range;
use std::sync::Arc;

use super::large::{ByteProgress, LargeHunk, LargeProgress};

/// Rows of a large card: the hunk count at which its scan stops as full (D26.4, D26.5).
pub const CARD_HUNKS: usize = 6;

/// Hex rows of a binary card: the region's own row, four above it and four below (D26.5).
pub const HEX_CARD_ROWS: usize = 9;

/// Longest side of a card thumbnail, in pixels; chosen to stay sharp at 200 % scale (D26.3).
pub const THUMB_SIDE: u32 = 240;

/// How a detail job ended. `Full` stops early once the card cannot show more (D26.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardScan {
    /// Still comparing; the snapshot is progress, not an answer.
    Running,
    /// The engine finished and the card shows all of its result.
    Done,
    /// The card reached what it can show; the engine stopped there.
    Full,
}

/// What a Stream card adds to its base card, replacing the previous detail of the id (D26.2).
/// Scores travel as whole percents and static notes so the whole type stays `Eq`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardDetail {
    /// The line ranges of a large text pair.
    Large(LargeCard),
    /// The first changed region of a binary pair, as hex.
    Bytes(BytesCard),
    /// The thumbnails and score of an image pair.
    Image(ImageCard),
    /// Why the job shows nothing instead: the text for the card's note line.
    Failed(String),
}

/// A large card's body: its hunk rows and how far the scan got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeCard {
    /// At most `CARD_HUNKS` hunks, sorted by `left_rows.start`.
    pub hunks: Vec<LargeHunk>,
    /// Every hunk the scan has found so far, including those past the card's rows.
    pub found: u32,
    /// What the engine is doing, while `scan` is still `Running`.
    pub progress: Option<LargeProgress>,
    /// Whether the card is filling, finished, or stopped at a full set of hunks.
    pub scan: CardScan,
}

/// A binary card's body: the window around the first changed region (D26.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytesCard {
    /// The region's window; `None` while no region has been found yet.
    pub window: Option<HexWindow>,
    /// Byte-scan progress, while `scan` is still `Running`.
    pub progress: Option<ByteProgress>,
    /// `Full` once a region was found; `Done` when the files proved equal.
    pub scan: CardScan,
}

/// The bytes a binary card shows: `HEX_CARD_ROWS` rows of 16 on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexWindow {
    /// Byte offset of `left`'s first byte; a multiple of 16.
    pub left_start: u64,
    /// Byte offset of `right`'s first byte; a multiple of 16.
    pub right_start: u64,
    /// At most `HEX_CARD_ROWS` times 16 left bytes, starting at `left_start`.
    pub left: Vec<u8>,
    /// At most `HEX_CARD_ROWS` times 16 right bytes, starting at `right_start`.
    pub right: Vec<u8>,
    /// The changed bytes on the left, as absolute offsets into that file.
    pub changed_left: Range<u64>,
    /// The changed bytes on the right, as absolute offsets into that file.
    pub changed_right: Range<u64>,
    /// Per side: true when that file is over `u32::MAX` bytes, so its numbers need `u64`.
    pub wide: (bool, bool),
}

/// An image card's body: the score the card leads with and both thumbnails (D26.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCard {
    /// `similarity_of` the pair's scores, a whole percent; `None` until it is scored.
    pub similarity: Option<u32>,
    /// `notice_of` the pair's scores, a Fluent id; `None` when the pair was scored.
    pub note: Option<&'static str>,
    /// The left thumbnail; `None` when that side cannot be decoded within the caps.
    pub left: Option<Thumb>,
    /// The right thumbnail; `None` for the same reason.
    pub right: Option<Thumb>,
}

/// One card thumbnail: BGRA pixels whose longest side is at most `THUMB_SIDE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    /// Four bytes per pixel — blue, green, red, alpha — for `width` times `height` pixels.
    pub bgra: Arc<[u8]>,
}

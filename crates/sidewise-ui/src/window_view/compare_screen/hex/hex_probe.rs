//! What a hex body shows, for tests.

use std::ops::Range;

use super::HexView;
use super::fetch::CACHE_LIMIT;

/// The current region, visible rows, row count, alignment, gaps, fetched bytes and status.
#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct HexProbe {
    #[doc(hidden)]
    pub current: Option<usize>,
    #[doc(hidden)]
    pub built: Range<usize>,
    #[doc(hidden)]
    pub rows: usize,
    #[doc(hidden)]
    pub regions: usize,
    #[doc(hidden)]
    pub run: u64,
    #[doc(hidden)]
    pub align: &'static str,
    #[doc(hidden)]
    pub gaps: Vec<bool>,
    #[doc(hidden)]
    pub loaded: Vec<bool>,
    #[doc(hidden)]
    pub ready: bool,
    /// The most spans either side's byte cache holds, and the size that triggers eviction.
    #[doc(hidden)]
    pub cached: usize,
    #[doc(hidden)]
    pub cache_limit: usize,
    /// The left byte offset the top row starts at; none on a gap.
    #[doc(hidden)]
    pub top_left: Option<u64>,
    /// The right byte offset the top row starts at; none when the right side has a gap.
    #[doc(hidden)]
    pub top_right: Option<u64>,
    #[doc(hidden)]
    pub left_hex: Vec<Option<String>>,
    #[doc(hidden)]
    pub status: Option<String>,
}

impl HexView {
    /// The current region, visible rows, row count, alignment and gap rows, for tests.
    #[doc(hidden)]
    pub fn probe(&mut self, visible: usize) -> HexProbe {
        let range = self.viewport(visible);
        HexProbe {
            current: self.current,
            built: range.clone(),
            rows: self.rows.count(),
            regions: self.regions.len(),
            run: self.run,
            align: self.align.name(),
            gaps: range
                .clone()
                .map(|at| self.rows.row(at).is_some_and(|row| row.is_gap()))
                .collect(),
            status: self.status().map(|text| text.to_string()),
            cached: self.loaded.len(),
            cache_limit: CACHE_LIMIT,
            top_left: self.rows.left_at(range.start),
            top_right: self
                .rows
                .row(range.start)
                .and_then(|row| row.right)
                .map(|span| span.at),
            ready: self.left.is_some() || self.right.is_some() || self.error.is_some(),
            loaded: range
                .clone()
                .map(|at| self.rows.row(at).is_some_and(|row| self.loaded.covers(row)))
                .collect(),
            left_hex: range
                .map(|at| {
                    let span = self.rows.row(at)?.left;
                    Some(self.loaded.left(span)?.hex.to_string())
                })
                .collect(),
        }
    }
}

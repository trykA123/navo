//! What a hex body shows, for tests.

use std::ops::Range;

use super::HexView;
use super::fetch::CACHE_LIMIT;

/// The current region, visible rows, row count, alignment, gaps, fetched bytes and status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view) struct HexProbe {
    pub(in crate::window_view) current: Option<usize>,
    pub(in crate::window_view) built: Range<usize>,
    pub(in crate::window_view) rows: usize,
    pub(in crate::window_view) regions: usize,
    pub(in crate::window_view) run: u64,
    pub(in crate::window_view) align: &'static str,
    pub(in crate::window_view) gaps: Vec<bool>,
    pub(in crate::window_view) loaded: Vec<bool>,
    pub(in crate::window_view) ready: bool,
    /// The most spans either side's byte cache holds, and the size that triggers eviction.
    pub(in crate::window_view) cached: usize,
    pub(in crate::window_view) cache_limit: usize,
    pub(in crate::window_view) left_hex: Vec<Option<String>>,
    pub(in crate::window_view) status: Option<String>,
}

impl HexView {
    /// The current region, visible rows, row count, alignment and gap rows, for tests.
    pub(in crate::window_view::compare_screen) fn probe(&mut self, visible: usize) -> HexProbe {
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

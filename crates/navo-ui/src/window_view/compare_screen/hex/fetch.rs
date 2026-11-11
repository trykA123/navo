//! Background reads of the hex body (V39): opening both sides once, fetching the visible rows,
//! and the per-side cache of fetched rows, keyed by span so a moved row never shows old bytes.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use navo_core::Location;
use navo_core::paging::{ByteSource, HexRow, hex_row, read_full};
use navo_l10n::tr;

use super::super::super::FileHooks;
use super::HexView;
use super::hex_rows::{RowAt, RowSpan};
use crate::compare_diff::PairRequest;
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// How many cached spans trigger an eviction; the shown rows and the viewport at the current top are kept.
pub(super) const CACHE_LIMIT: usize = 4096;

/// The two byte sources of a hex body, or the first reason one could not open.
pub(in crate::window_view::compare_screen) struct SidesOpened {
    pub(super) left: Option<Arc<dyn ByteSource + Send + Sync>>,
    pub(super) right: Option<Arc<dyn ByteSource + Send + Sync>>,
    pub(super) error: Option<SharedString>,
}

/// Both sides' sources, used on the background read only.
#[derive(Clone)]
pub(in crate::window_view::compare_screen) struct Sources {
    left: Option<Arc<dyn ByteSource + Send + Sync>>,
    right: Option<Arc<dyn ByteSource + Send + Sync>>,
    left_wide: bool,
    right_wide: bool,
}

/// One span and its row, or `None` when the read failed.
type ReadSpan = (RowSpan, Option<HexRow>);

/// What one background read brought back, per side.
pub(in crate::window_view::compare_screen) struct Fetched {
    left: Vec<ReadSpan>,
    right: Vec<ReadSpan>,
}

/// Rows already read, per side: a cache whose key is the span, so a hit is always that span.
#[derive(Default)]
pub(super) struct ByteCache {
    left: HashMap<RowSpan, Option<HexRow>>,
    right: HashMap<RowSpan, Option<HexRow>>,
}

impl ByteCache {
    /// Whether both sides of `row` were read; a failed read counts, so it is not retried.
    pub(super) fn covers(&self, row: RowAt) -> bool {
        row.left.is_none_or(|span| self.left.contains_key(&span))
            && row.right.is_none_or(|span| self.right.contains_key(&span))
    }

    /// The left row of exactly `span`, if it was read.
    pub(super) fn left(&self, span: Option<RowSpan>) -> Option<HexRow> {
        self.left.get(&span?)?.clone()
    }

    /// The right row of exactly `span`, if it was read.
    pub(super) fn right(&self, span: Option<RowSpan>) -> Option<HexRow> {
        self.right.get(&span?)?.clone()
    }

    /// Forgets every row except the spans of `keep`.
    fn retain_rows(&mut self, keep: impl Iterator<Item = RowAt>) {
        let mut kept = Self::default();
        for row in keep {
            if let Some((span, bytes)) = row.left.and_then(|span| self.left.remove_entry(&span)) {
                kept.left.insert(span, bytes);
            }
            if let Some((span, bytes)) = row.right.and_then(|span| self.right.remove_entry(&span)) {
                kept.right.insert(span, bytes);
            }
        }
        *self = kept;
    }

    /// Forgets every row.
    pub(super) fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
    }

    /// The most spans either side holds, for tests.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.left.len().max(self.right.len())
    }

    /// Whether the cache grew past its limit.
    fn is_full(&self) -> bool {
        self.left.len().max(self.right.len()) > CACHE_LIMIT
    }

    /// Keeps the rows of one background read.
    fn store(&mut self, fetched: Fetched) {
        self.left.extend(fetched.left);
        self.right.extend(fetched.right);
    }
}

/// Reads the spans of `reads` off the UI thread.
pub(in crate::window_view::compare_screen) fn fetch_rows(
    sources: &Sources,
    reads: &[RowAt],
) -> Fetched {
    let (left, right) = (sources.left.as_deref(), sources.right.as_deref());
    Fetched {
        left: reads
            .iter()
            .filter_map(|row| row.left)
            .map(|span| (span, read_side(left, span, sources.left_wide)))
            .collect(),
        right: reads
            .iter()
            .filter_map(|row| row.right)
            .map(|span| (span, read_side(right, span, sources.right_wide)))
            .collect(),
    }
}

/// Reads one side's span, or `None` for a missing side or a read error.
fn read_side(
    source: Option<&(dyn ByteSource + Send + Sync)>,
    span: RowSpan,
    wide: bool,
) -> Option<HexRow> {
    let mut bytes = [0u8; 16];
    let buf = bytes.get_mut(..span.len().min(16))?;
    let read = read_full(source?, span.at, buf).ok()?;
    Some(hex_row(span.at, bytes.get(..read)?, wide))
}

/// Opens one side through the viewer's paged hook, or keeps the reason it failed.
fn side(
    location: Option<&Location>,
    files: &FileHooks,
) -> (
    Option<Arc<dyn ByteSource + Send + Sync>>,
    Option<SharedString>,
) {
    let Some(location) = location else {
        return (None, Some(tr("compare-hex-no-side").into()));
    };
    match (files.paged)(location) {
        Ok(paged) => (Some(paged.source), None),
        Err(error) => (None, Some(error.into())),
    }
}

impl HexView {
    /// Opens both sides through the viewer's paged hooks; runs on a background thread.
    pub(in crate::window_view::compare_screen) fn open_sides(
        request: &PairRequest,
        files: &FileHooks,
    ) -> SidesOpened {
        let (left, left_error) = side(request.left.as_ref(), files);
        let (right, right_error) = side(request.right.as_ref(), files);
        SidesOpened {
            left,
            right,
            error: left_error.or(right_error),
        }
    }

    /// Claims this view's one open: true the first time, false ever after, even after a failure.
    pub(in crate::window_view::compare_screen) fn begin_open(&mut self) -> bool {
        !std::mem::replace(&mut self.open_started, true)
    }

    /// Stores the open task so dropping the view cancels it.
    pub(in crate::window_view::compare_screen) fn set_open_task(
        &mut self,
        task: gpui_kit::Task<()>,
    ) {
        self.open = Some(task);
    }

    /// Whether a row of `range` has no bytes for its current span.
    pub(super) fn unfetched(&self, range: &Range<usize>) -> bool {
        range
            .clone()
            .filter_map(|at| self.rows.row(at))
            .any(|row| !self.loaded.covers(row))
    }

    /// Whether `range` needs a read: a row is unfetched and no read of `range` is running.
    fn needs_rows(&self, range: &Range<usize>) -> bool {
        self.pending.as_ref() != Some(range) && self.unfetched(range)
    }

    /// The sources and spans to read for `range`; notes it as shown and marks it pending.
    pub(in crate::window_view::compare_screen) fn begin_fetch(
        &mut self,
        range: Range<usize>,
    ) -> Option<(Sources, Vec<RowAt>, Range<usize>)> {
        self.shown = range.clone();
        self.release_held(&range);
        if !self.needs_rows(&range) {
            return None;
        }
        let reads = range
            .clone()
            .filter_map(|index| self.rows.row(index))
            .collect();
        let sources = Sources {
            left: self.left.clone(),
            right: self.right.clone(),
            left_wide: self.left_wide,
            right_wide: self.right_wide,
        };
        self.pending = Some(range.clone());
        Some((sources, reads, range))
    }

    /// Keeps one fetched range, evicting all but the shown rows and the viewport at the current
    /// top when full; another range's read is dropped. Its bytes are keyed by span, so a read that outlived a row-map change is
    /// still correct and only fills less.
    pub(in crate::window_view::compare_screen) fn install_rows(
        &mut self,
        range: Range<usize>,
        fetched: Fetched,
    ) {
        if self.pending.as_ref() != Some(&range) {
            return;
        }
        self.pending = None;
        // The held map draws from this cache, so it is never evicted under one. The rows on
        // screen stay (the last requested ones and those at the current top, which a scroll
        // moved before any request), so a late read never blanks them.
        if self.held.is_none() && self.loaded.is_full() {
            let now = self.viewport(self.shown.len());
            let keep = self.shown.clone().chain(now);
            self.loaded
                .retain_rows(keep.filter_map(|at| self.rows.row(at)));
        }
        self.loaded.store(fetched);
        self.release_held(&range);
    }

    /// Stores the running fetch so dropping the view cancels it.
    pub(in crate::window_view::compare_screen) fn set_fetch_task(
        &mut self,
        task: gpui_kit::Task<()>,
    ) {
        self.fetch = Some(task);
    }

    /// Asks for the visible rows after this frame, so render does no I/O and changes no state.
    pub(in crate::window_view::compare_screen) fn request_later(
        &self,
        window: &Window,
        cx: &mut Context<NavoWindow>,
    ) {
        let weak = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| view.request_hex_rows(window, cx));
            }
        });
    }
}

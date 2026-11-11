//! The hex body of the file diff (V39, D18): two F3-style hex columns, changed-byte tints,
//! gap rows in content alignment, and next and previous region.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::Scrollbar;
use gpui_kit::{ScrollEvent, canvas};
use navo_core::paging::ByteSource;
use navo_l10n::{t, tr};

use self::draw::{Tints, View, message, rows};
use self::fetch::ByteCache;
pub(super) use self::fetch::{Fetched, SidesOpened, fetch_rows};
use self::held::Held;
pub(super) use self::hex_rows::Rows;
pub(super) use self::seek::Anchor;
use crate::compare_diff::{ByteRegionView, HexAlign};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::ByteScroll;

mod draw;
mod fetch;
mod held;
mod hex_rows;
mod scroll;
mod seek;

#[cfg(test)]
pub(in crate::window_view) use self::hex_probe::HexProbe;
#[cfg(test)]
mod hex_probe;

/// How many rows the wheel moves per line.
const WHEEL_ROWS: i64 = 3;

/// An opened binary pair as the file diff shows it.
pub(super) struct HexView {
    left: Option<Arc<dyn ByteSource + Send + Sync>>,
    right: Option<Arc<dyn ByteSource + Send + Sync>>,
    left_wide: bool,
    right_wide: bool,
    lengths: (u64, u64),
    regions: Vec<ByteRegionView>,
    left_marks: Vec<(Range<u64>, usize)>,
    right_marks: Vec<(Range<u64>, usize)>,
    rows: Rows,
    /// The drawn state still on screen while the current map's bytes are on the way.
    held: Option<Held>,
    align: HexAlign,
    /// The user's alignment; a late install never overwrites it.
    user_align: Option<HexAlign>,
    /// The run the current regions belong to.
    run: u64,
    /// The alignment regions were requested for, so install sends one request only.
    requested: Option<HexAlign>,
    /// Fractional wheel rows kept between events.
    wheel_remainder: f64,
    current: Option<usize>,
    /// The first visible row, as a count, never a pixel offset.
    top: usize,
    loaded: ByteCache,
    /// The rows on screen at the last request; eviction keeps them and the viewport at `top`.
    shown: Range<usize>,
    /// The range a background read is fetching.
    pending: Option<Range<usize>>,
    /// The running read; dropping it cancels the fetch.
    fetch: Option<gpui_kit::Task<()>>,
    /// The background open of both sides; dropping it cancels.
    open: Option<gpui_kit::Task<()>>,
    scroll: ByteScroll,
    error: Option<SharedString>,
    /// The byte offset to put on top once the rows exist (Alt+V Bytes; Show bytes here seeks
    /// the right file for a line only it has).
    start: Option<Anchor>,
    /// The exact left offset the top row was sought, kept while that row stays on top so a
    /// rebuild (regions landing) puts the same byte back on top.
    pin: Option<u64>,
    /// Set by the view's one open and never cleared, so no wake or failure opens again.
    open_started: bool,
}

impl HexView {
    /// A hex body whose two sides are still to be read on a background thread.
    pub(super) fn new(user_align: Option<HexAlign>, run: u64) -> Self {
        Self {
            left: None,
            right: None,
            left_wide: false,
            right_wide: false,
            lengths: (0, 0),
            regions: Vec::new(),
            left_marks: Vec::new(),
            right_marks: Vec::new(),
            rows: Rows::offset((0, 0)),
            held: None,
            align: user_align.unwrap_or(HexAlign::Offset),
            user_align,
            run,
            requested: None,
            wheel_remainder: 0.,
            current: None,
            top: 0,
            loaded: ByteCache::default(),
            shown: 0..0,
            pending: None,
            fetch: None,
            open: None,
            scroll: ByteScroll::default(),
            error: None,
            start: None,
            pin: None,
            open_started: false,
        }
    }

    /// Installs the opened sides and lays out their rows, keeping the user's alignment.
    pub(super) fn install(&mut self, opened: SidesOpened) {
        self.left = opened.left;
        self.right = opened.right;
        self.error = opened.error;
        self.lengths = (
            self.left.as_ref().map_or(0, |source| source.len()),
            self.right.as_ref().map_or(0, |source| source.len()),
        );
        self.left_wide = self.lengths.0 > u64::from(u32::MAX);
        self.right_wide = self.lengths.1 > u64::from(u32::MAX);
        let default = match self.lengths.0 == self.lengths.1 {
            true => HexAlign::Offset,
            false => HexAlign::Content,
        };
        self.align = self.user_align.unwrap_or(default);
        // A refreshed view keeps the old pair drawn through the open; a failure shows its message.
        let old_pair = self
            .held
            .take()
            .filter(|held| held.is_carried() && self.error.is_none());
        self.rebuild();
        self.apply_start();
        self.reset_loading();
        self.held = old_pair;
    }

    /// Adds the changed regions of one `Bytes` snapshot; false when it adds nothing, is stale,
    /// or was never asked for, as a run inherited across a refresh before the open lands.
    pub(super) fn add(
        &mut self,
        regions: Vec<ByteRegionView>,
        (align, run): (HexAlign, u64),
    ) -> bool {
        if regions.is_empty() || self.requested != Some(align) || run != self.run {
            return false;
        }
        let first = regions[0].left.start;
        let last = self.regions.last().map_or(0, |region| region.left.start);
        self.regions.extend(regions);
        if first < last {
            self.sort_regions();
        }
        if self.align == HexAlign::Offset {
            self.left_marks = draw::marks(&self.regions, false);
            self.right_marks = draw::marks(&self.regions, true);
            if self.current.is_some_and(|at| at >= self.regions.len()) {
                self.current = self.regions.len().checked_sub(1);
            }
        } else {
            self.rebuild();
        }
        true
    }

    /// Switches alignment and drops the old regions; the caller requests regions for the new one.
    pub(super) fn set_align(&mut self, align: HexAlign) -> bool {
        if self.align == align && self.user_align == Some(align) {
            return false;
        }
        self.user_align = Some(align);
        self.align = align;
        self.requested = None;
        self.regions.clear();
        self.current = None;
        self.rebuild();
        true
    }

    /// The byte lengths of the two sides; zero for a side that is missing.
    pub(super) fn lengths(&self) -> (u64, u64) {
        self.lengths
    }

    /// The current alignment.
    pub(super) fn align(&self) -> HexAlign {
        self.align
    }

    /// Issues the run for the current alignment, once per request.
    pub(super) fn request_regions(&mut self) -> Option<u64> {
        if self.requested == Some(self.align) {
            return None;
        }
        self.run = self.run.saturating_add(1);
        self.requested = Some(self.align);
        Some(self.run)
    }

    /// The status line: "Region i of n", "n regions" or "No changes"; none while bytes fail.
    pub(super) fn status(&self) -> Option<SharedString> {
        let total = self.regions.len();
        if self.error.is_some() || (total == 0 && self.shows_old_pair()) {
            return None;
        }
        Some(match (self.current, total) {
            (_, 0) => tr("compare-screen-no-changes").into(),
            (Some(at), _) => t!("compare-hex-region", current = at + 1, total = total).into(),
            (None, _) => t!("compare-hex-regions", total = total).into(),
        })
    }

    /// How many regions the pair has.
    pub(super) fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Moves to the next (`forward`) or previous region and centres it; stops at the ends.
    pub(super) fn step(&mut self, forward: bool, visible: usize) -> bool {
        let Some(last) = self.regions.len().checked_sub(1) else {
            return false;
        };
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => last,
            (Some(at), true) => (at + 1).min(last),
            (Some(at), false) => at.saturating_sub(1),
        };
        if self.current == Some(next) {
            return false;
        }
        self.current = Some(next);
        if let Some(row) = self.rows.row_for_region(next, &self.regions, self.align) {
            let last_page = self.row_count().saturating_sub(visible.max(1));
            self.top = row.saturating_sub(visible / 2).min(last_page);
        }
        true
    }

    /// The visible row range at `top`.
    pub(super) fn viewport(&self, visible: usize) -> Range<usize> {
        let top = self
            .top
            .min(self.row_count().saturating_sub(visible.max(1)));
        top..top.saturating_add(visible)
    }

    /// The body: visible rows, the scrollbar and the wheel; or a side's failure message.
    pub(super) fn body(
        &self,
        t: &Theme,
        window: &Window,
        cx: &mut Context<NavoWindow>,
    ) -> AnyElement {
        if let Some(error) = &self.error {
            return message(t, error.clone());
        }
        let visible = visible_rows(t, window);
        let range = self.viewport(visible);
        self.request_later(window, cx);
        let measure = self.measure();
        let drawn = self.drawn(&range);
        let body = rows(View {
            t,
            rows: drawn.rows,
            range,
            loaded: drawn.loaded,
            tints: Tints {
                left: drawn.left,
                right: drawn.right,
                current: drawn.current,
            },
        });
        div()
            .id("compare-hex-body")
            .aria_label(tr("compare-hex-rows"))
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .on_scroll(cx.listener(|view, event: &ScrollEvent, window, cx| {
                view.scroll_hex(event, window, cx);
            }))
            .child(measure)
            .children(body)
            .child(self.scrollbar())
            .into_any_element()
    }

    /// The scrollbar along the body's right edge.
    fn scrollbar(&self) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .child(Scrollbar::vertical(&self.scroll))
    }

    /// The canvas that gives the scrollbar its viewport bounds.
    fn measure(&self) -> impl IntoElement {
        let scroll = self.scroll.clone();
        canvas(
            move |bounds, _, _| {
                scroll.measure(bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// Drops fetched bytes, any fetch and the held state when the pair's sources change.
    fn reset_loading(&mut self) {
        self.held = None;
        self.loaded.clear();
        self.pending = None;
        self.fetch = None;
    }

    /// Sorts the regions by offset and keeps `current` on the same region.
    fn sort_regions(&mut self) {
        let key = |region: &ByteRegionView| (region.left.start, region.right.start);
        let current = self.current.and_then(|at| self.regions.get(at)).map(key);
        self.regions.sort_by_key(key);
        if let Some(current) = current {
            self.current = self.regions.binary_search_by_key(&current, key).ok();
        }
    }

    /// Rebuilds rows and tint indexes; the old map stays on screen until the new bytes land.
    fn rebuild(&mut self) {
        let anchor = self.anchor();
        let rows = match self.align {
            HexAlign::Offset => Rows::offset(self.lengths),
            HexAlign::Content => Rows::content(self.lengths, &self.regions),
        };
        let shown = std::mem::replace(&mut self.rows, rows);
        self.held.get_or_insert(Held::shared(shown));
        self.left_marks = draw::marks(&self.regions, false);
        self.right_marks = draw::marks(&self.regions, true);
        if self.installed() {
            self.top = self.top.min(self.rows.count().saturating_sub(1));
        }
        self.reanchor(anchor);
        if self.current.is_some_and(|at| at >= self.regions.len()) {
            self.current = self.regions.len().checked_sub(1);
        }
    }
}

/// How many rows fit between the header and the status line.
pub(super) fn visible_rows(t: &Theme, window: &Window) -> usize {
    let height = window.viewport_size().height - t.spacing.path_height * 2.;
    let rows = height / t.spacing.row_height;
    rows.ceil().max(1.) as usize
}

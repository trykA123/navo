//! Draws the large text body: a virtual list over the window of rows from the bar's base, cut
//! at the shared sideways offset (W8j) with the strip under it. It only reads the row map and
//! the lines that arrived; asking for lines happens in event handlers.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use navo_l10n::tr;

use gpui_kit::WeakEntity;

use super::super::context::{RowHit, on_right_click};
use super::super::sideways::{SidewaysHandle, advance, on_wheel};
use super::super::strip::strip;
use super::super::switch;
use super::map::RowMap;
use super::{LargeShown, LineCache};
use crate::compare_diff::body::{BodyLook, row_view_cut};
use crate::compare_diff::{OnBytesHere, ShownRow};
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens;
use crate::window_view::ScreenOwner;

/// Everything the list's rows closure holds between frames.
struct ListRows {
    map: Rc<RowMap>,
    lines: Rc<RefCell<LineCache>>,
    /// The first row of the list's window; rows are numbered from it.
    base: u64,
    current: Option<u32>,
    digits: usize,
    on_long: OnBytesHere,
    click: WeakEntity<ScreenOwner>,
    /// The sideways offset every row's text is cut by; the first row carries its probe.
    sideways: SidewaysHandle,
}

impl ListRows {
    /// Draws list items `range`, cut at this frame's sideways offset; a line over `LONG_LINE`
    /// draws whole with its "Show bytes here" button, as in the file diff. The first item's
    /// text cells report the probe the cut and the strip are measured against.
    fn draw(&self, range: Range<usize>, window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
        let cache = self.lines.borrow();
        let look = BodyLook {
            theme: tokens::current(cx),
            layout: self.map.layout(),
            digits: self.digits,
            current: self.current,
            spans: None,
            ticks: None,
            bytes_here: Some(&self.on_long),
            syntax: None,
        };
        let cut = self.sideways.frame(advance(window, look.theme));
        let probe = self.sideways.probe();
        let row = |at: usize| {
            self.base
                .saturating_add(u64::try_from(at).unwrap_or(u64::MAX))
        };
        range
            .enumerate()
            .map(|(i, at)| {
                let mut cut = cut.clone();
                if i == 0 {
                    cut.probe = Some(probe.clone());
                }
                let shown = shown(&self.map, &cache, row(at));
                on_right_click(
                    &self.click,
                    row_view_cut(&look, &shown, &cut),
                    RowHit::of(&shown),
                )
            })
            .collect()
    }
}

impl LargeShown {
    /// The row list in the map's layout over the sideways strip; it captures plain data only,
    /// never the pair control.
    pub(in crate::window_view::compare_screen) fn list(
        &self,
        weak: &WeakEntity<ScreenOwner>,
        cx: &App,
    ) -> AnyElement {
        let row_height = tokens::current(cx).spacing.row_height;
        let current = self.current_index().and_then(|at| u32::try_from(at).ok());
        let count = usize::try_from(self.window_len()).unwrap_or(usize::MAX);
        let digits = self
            .map
            .totals()
            .map_or(self.map.count(), |(left, right)| left.max(right))
            .max(1)
            .to_string()
            .len();
        let rows = ListRows {
            map: Rc::clone(&self.map),
            lines: Rc::clone(&self.lines),
            base: self.bar.base(),
            current,
            digits,
            on_long: switch::on_bytes_here(weak),
            click: weak.clone(),
            sideways: self.sideways.clone(),
        };
        let wheel = on_wheel(rows.sideways.clone(), weak.clone());
        let rows = move |range: Range<usize>, window: &mut Window, cx: &mut App| {
            rows.draw(range, window, cx)
        };
        let list = VirtualList::new("compare-large-rows", count, &self.scroll, rows)
            .row_height(row_height)
            .scrollbar(&self.bar)
            .label(tr("compare-screen-rows"))
            .axis_locked();
        div()
            .flex()
            .flex_col()
            .size_full()
            .on_scroll(wheel)
            .child(list)
            .child(strip(&self.sideways, self.map.layout()))
            .into_any_element()
    }
}

/// What list row `row` shows: its kind and lines; a line that has not arrived reads empty.
fn shown<'a>(map: &RowMap, cache: &'a LineCache, row: u64) -> ShownRow<'a> {
    let shown = map.at(row);
    let side = |left: bool, line: Option<u64>| {
        line.map(|at| {
            let number = u32::try_from(at).unwrap_or(u32::MAX);
            (number, cache.line(left, at).unwrap_or_default())
        })
    };
    ShownRow {
        kind: shown.kind,
        left: side(true, shown.left),
        right: side(false, shown.right),
        hunk: shown.hunk.and_then(|n| u32::try_from(n).ok()),
        row: u32::try_from(row).unwrap_or(u32::MAX),
    }
}

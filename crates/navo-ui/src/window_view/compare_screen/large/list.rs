//! Draws the large text body: a virtual list over the window of rows from the bar's base. It only
//! reads the row map and the lines that arrived; asking for lines happens in event handlers.

use std::ops::Range;
use std::rc::Rc;

use navo_l10n::tr;

use gpui_kit::WeakEntity;

use super::super::switch;
use super::map::RowMap;
use super::{LargeShown, LineCache};
use crate::compare_diff::ShownRow;
use crate::compare_diff::body::{BodyLook, row_view};
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens;
use crate::window_view::NavoWindow;

impl LargeShown {
    /// The row list in the map's layout; it captures plain data only, never the pair control.
    pub(in crate::window_view::compare_screen) fn list(
        &self,
        weak: &WeakEntity<NavoWindow>,
        cx: &App,
    ) -> AnyElement {
        let row_height = tokens::current(cx).spacing.row_height;
        let (map, lines, base) = (
            Rc::clone(&self.map),
            Rc::clone(&self.lines),
            self.bar.base(),
        );
        let current = self.current_index().and_then(|at| u32::try_from(at).ok());
        let count = usize::try_from(self.window_len()).unwrap_or(usize::MAX);
        let digits = map
            .totals()
            .map_or(map.count(), |(left, right)| left.max(right))
            .max(1)
            .to_string()
            .len();
        let on_long = switch::on_bytes_here(weak);
        let rows = move |range: Range<usize>, _: &mut Window, cx: &mut App| {
            let cache = lines.borrow();
            let look = BodyLook {
                theme: tokens::current(cx),
                layout: map.layout(),
                digits,
                current,
                spans: None,
                ticks: None,
                bytes_here: Some(&on_long),
            };
            let row = |at: usize| base.saturating_add(u64::try_from(at).unwrap_or(u64::MAX));
            range.map(|at| draw(&look, &map, &cache, row(at))).collect()
        };
        VirtualList::new("compare-large-rows", count, &self.scroll, rows)
            .row_height(row_height)
            .scrollbar(&self.bar)
            .label(tr("compare-screen-rows"))
            .into_any_element()
    }
}

/// Draws list row `row`; a line that has not arrived yet shows its number only.
fn draw(look: &BodyLook, map: &RowMap, cache: &LineCache, row: u64) -> AnyElement {
    let shown = map.at(row);
    let side = |left: bool, line: Option<u64>| {
        line.map(|at| {
            let number = u32::try_from(at).unwrap_or(u32::MAX);
            (number, cache.line(left, at).unwrap_or_default())
        })
    };
    let drawn = ShownRow {
        kind: shown.kind,
        left: side(true, shown.left),
        right: side(false, shown.right),
        hunk: shown.hunk.and_then(|n| u32::try_from(n).ok()),
        row: u32::try_from(row).unwrap_or(u32::MAX),
    };
    row_view(look, &drawn)
}

//! Draws the large text body: a virtual list over the row map. Each frame's rows ask the app
//! for the lines they lack, one window at a time, and move the diff's focus there (D16).

use std::ops::Range;
use std::rc::Rc;

use navo_l10n::tr;

use super::map::RowMap;
use super::{LargeShown, LineCache};
use crate::compare_diff::body::{BodyLook, row_view};
use crate::compare_diff::{Layout, PairControl, ShownRow};
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens;

impl LargeShown {
    /// The row list in `layout`; rows without their lines yet ask `control` for a window.
    pub(in crate::window_view::compare_screen) fn list(
        &mut self,
        layout: Layout,
        control: &Rc<dyn PairControl>,
        cx: &App,
    ) -> AnyElement {
        let row_height = tokens::current(cx).spacing.row_height;
        if self.map.layout() != layout {
            let map = RowMap::new(&self.hunks, layout, self.map.totals());
            self.remap(map);
        }
        self.row_height = row_height;
        let (map, lines, control) = (
            Rc::clone(&self.map),
            Rc::clone(&self.lines),
            Rc::clone(control),
        );
        let current = self.current_index().and_then(|at| u32::try_from(at).ok());
        let count = usize::try_from(map.count()).unwrap_or(usize::MAX);
        let digits = map
            .totals()
            .map_or(map.count(), |(left, right)| left.max(right))
            .max(1)
            .to_string()
            .len();
        let rows = move |range: Range<usize>, _: &mut Window, cx: &mut App| {
            let rows = u64::try_from(range.start).unwrap_or(u64::MAX)
                ..u64::try_from(range.end).unwrap_or(u64::MAX);
            ask(&map, &lines, control.as_ref(), rows.clone());
            let cache = lines.borrow();
            let look = BodyLook {
                theme: tokens::current(cx),
                layout,
                digits,
                current,
                spans: None,
                ticks: None,
            };
            rows.map(|row| draw(&look, &map, &cache, row)).collect()
        };
        VirtualList::new("compare-large-rows", count, &self.scroll, rows)
            .row_height(row_height)
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

/// Asks `control` for the first missing window of `rows` unless it is in flight, and moves the
/// diff's focus to the middle row's line so its section is diffed first.
fn ask(
    map: &RowMap,
    lines: &std::cell::RefCell<LineCache>,
    control: &dyn PairControl,
    rows: Range<u64>,
) {
    let wanted = lines.borrow().wanted(map, rows.clone());
    let Some(window) = wanted else {
        return;
    };
    lines.borrow_mut().asked(window.clone());
    let middle = map.at(rows.start + (rows.end - rows.start) / 2);
    control.focus(middle.left.or(middle.right).unwrap_or(window.start));
    control.lines(window);
}

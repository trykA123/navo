//! The folder compare's change map (D36): the ticks its rows make, and the narrow strip at the
//! trailing edge of the pane that shows the compare's right side. It reuses the file diff's map.

#[cfg(test)]
mod tests;

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{EntityId, Subscription};

use crate::browser::{RowView, Tone};
use crate::compare_diff::map::{ChangeMap, HunkKind, MapEvent, MapHunk};
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::tokens;
use crate::window_view::NavoWindow;

/// Width of the folder map's strip, in px: the compare panes are half a window wide.
const WIDTH: Pixels = px(12.);
/// The pane that draws the map: the one showing the compare's right side.
const SIDE: usize = 1;

/// The kind of tick a tone makes; `None` where the row shows no difference.
fn kind_of(tone: Tone) -> Option<HunkKind> {
    match tone {
        Tone::Add => Some(HunkKind::Added),
        Tone::Delete => Some(HunkKind::Deleted),
        Tone::Change | Tone::Conflict => Some(HunkKind::Changed),
        Tone::Muted | Tone::Busy => None,
    }
}

/// The map hunks of one pane's rows: one tick per difference row, at that row's own index.
fn folder_hunks(rows: &[RowView]) -> Arc<[MapHunk]> {
    (0u32..)
        .zip(rows)
        .filter_map(|(at, row)| {
            Some(MapHunk {
                rows: at..at + 1,
                kind: kind_of(row.tone)?,
            })
        })
        .collect()
}

/// The tick that shows the selected row: the count of difference rows before it. `None` when that
/// row is quiet, busy, or past the end of the list.
fn current_hunk(rows: &[RowView], selected: usize) -> Option<usize> {
    let difference = |row: &RowView| kind_of(row.tone).is_some();
    let row = rows.get(selected)?;
    difference(row).then(|| {
        rows[..selected]
            .iter()
            .filter(|row| difference(row))
            .count()
    })
}

/// The rows the shared scroll shows now, out of `total` rows `row_height` px high.
fn visible_rows(scroll: &ListScroll, row_height: Pixels, total: u32) -> Range<u32> {
    if row_height <= px(0.) {
        return 0..0;
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(total);
    let first = count(scroll.top_row(row_height)).min(total);
    let shown = count(
        (f32::from(scroll.viewport_height()) / f32::from(row_height))
            .ceil()
            .max(0.) as usize,
    );
    first..first.saturating_add(shown).min(total)
}

/// The pane the map hangs on: the side it shows and the row the compare has selected.
pub(super) struct MapPane {
    pub(super) side: usize,
    pub(super) selected: Option<usize>,
}

/// What the map draws: the rows of the pane that shows it, the `built` they came from, and the
/// selected row.
struct MapFeed {
    key: (u64, u64),
    rows: Arc<[RowView]>,
    selected: Option<usize>,
}

/// The folder map: its entity, its jump subscription, and the `built` its hunks came from.
pub(super) struct MapMount {
    entity: Entity<ChangeMap>,
    _jumps: Subscription,
    key: (u64, u64),
}

impl MapMount {
    /// Builds the map over `rows` and subscribes to the jumps it reports.
    fn new(rows: &[RowView], key: (u64, u64), cx: &mut Context<NavoWindow>) -> Self {
        let total = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let entity = cx.new(|_| ChangeMap::new(folder_hunks(rows), total).with_width(WIDTH));
        let _jumps = cx.subscribe(&entity, |view, _, event: &MapEvent, cx| {
            view.map_jump(event, cx);
        });
        Self {
            entity,
            _jumps,
            key,
        }
    }
}

/// The strip's element: registered by id, as tall as the list it stands beside.
fn strip(entity: Entity<ChangeMap>) -> AnyElement {
    div()
        .id("compare-folder-map")
        .test_support()
        .flex_none()
        .h_full()
        .child(entity)
        .into_any_element()
}

impl NavoWindow {
    /// `list` with the change map at its trailing edge; only the pane showing the compare's right
    /// side gets one, and only while the compare has rows.
    pub(super) fn list_with_map(
        &mut self,
        pane: MapPane,
        list: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let feed = self.compare.built.as_ref().and_then(|built| {
            (pane.side == SIDE && !built.sides[SIDE].is_empty()).then(|| MapFeed {
                key: built.key,
                rows: Arc::clone(&built.sides[SIDE]),
                selected: pane.selected,
            })
        });
        let Some(feed) = feed else { return list };
        let map = self.map_strip(feed, cx);
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(list)
            .child(map)
            .into_any_element()
    }

    /// The map element, created on first use and fed this frame's hunks, rows on screen and
    /// selected tick.
    fn map_strip(&mut self, feed: MapFeed, cx: &mut Context<Self>) -> AnyElement {
        let total = u32::try_from(feed.rows.len()).unwrap_or(u32::MAX);
        let row_height = tokens::current(cx).rows.height;
        let viewport = visible_rows(&self.compare.scroll, row_height, total);
        let current = feed
            .selected
            .and_then(|selected| current_hunk(&feed.rows, selected));
        let mounted = self
            .compare
            .map
            .get_or_insert_with(|| MapMount::new(&feed.rows, feed.key, cx));
        if mounted.key != feed.key {
            mounted.key = feed.key;
            let hunks = folder_hunks(&feed.rows);
            mounted
                .entity
                .update(cx, |map, cx| map.set_hunks(hunks, total, cx));
        }
        mounted.entity.update(cx, |map, cx| {
            map.set_viewport(viewport, cx);
            map.set_current(current, cx);
        });
        strip(mounted.entity.clone())
    }

    /// The map asked to show `row`: the shared scroll centres it and both panes repaint. The
    /// compare's selection stays where it was.
    fn map_jump(&mut self, event: &MapEvent, cx: &mut Context<Self>) {
        let MapEvent::Jump(row) = *event;
        self.compare.scroll.scroll_to_center(row as usize);
        let panes: Vec<EntityId> = self.cached.pane_ids().collect();
        cx.defer(move |cx| {
            for id in panes {
                cx.notify(id);
            }
        });
    }
}

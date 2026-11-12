//! Mounts the change map on the file diff: feeds it the hunks, the rows on screen and the
//! current change, and scrolls the list when the map reports a jump.

use std::sync::Arc;

use gpui_kit::Subscription;

use super::FileView;
use crate::compare_diff::Layout;
use crate::compare_diff::map::{ChangeMap, HunkKind, MapEvent, MapHunk};
use crate::compare_diff::{DiffRow, PairText};
use crate::prelude::*;
use crate::window_view::ScreenOwner;

/// The map entity of one file view, the jump subscription and the layout it last got hunks for.
pub(super) struct MapMount {
    entity: Entity<ChangeMap>,
    _jumps: Subscription,
    layout: Layout,
}

/// The hunk kind the map colours: all inserted, all deleted, or else changed.
fn kind_of(rows: &[DiffRow]) -> HunkKind {
    let all = |pick: fn(&DiffRow) -> bool| !rows.is_empty() && rows.iter().all(pick);
    if all(|row| matches!(row, DiffRow::Inserted(_))) {
        HunkKind::Added
    } else if all(|row| matches!(row, DiffRow::Deleted(_))) {
        HunkKind::Deleted
    } else {
        HunkKind::Changed
    }
}

/// The map's hunks for `text` in `layout`: row ranges as the list shows them.
/// Unified shows a changed row as two rows, so later hunks start later.
pub(super) fn map_hunks(text: &PairText, layout: Layout) -> Arc<[MapHunk]> {
    let mut extra = 0;
    text.hunks
        .iter()
        .map(|hunk| {
            let rows = text
                .rows
                .get(hunk.rows.start as usize..hunk.rows.end as usize)
                .unwrap_or_default();
            let doubled = match layout {
                Layout::Unified => rows
                    .iter()
                    .filter(|row| matches!(row, DiffRow::Changed(..)))
                    .count() as u32,
                Layout::SideBySide => 0,
            };
            let start = hunk.rows.start + extra;
            extra += doubled;
            MapHunk {
                rows: start..hunk.rows.end + extra,
                kind: kind_of(rows),
            }
        })
        .collect()
}

impl FileView {
    /// The change map element, created on first use and fed the current state.
    pub(in crate::window_view::compare_screen) fn map_element(
        &mut self,
        layout: Layout,
        cx: &mut Context<ScreenOwner>,
    ) -> AnyElement {
        let total = u32::try_from(self.shape(layout).count).unwrap_or(u32::MAX);
        let hunks = map_hunks(&self.text, layout);
        let (start, end) = self.built.get();
        let viewport = start as u32..end as u32;
        let current = self.current;
        let mount = self.map.get_or_insert_with(|| {
            let entity = cx.new(|_| ChangeMap::new(Arc::clone(&hunks), total));
            let _jumps = cx.subscribe(&entity, |window, _, event: &MapEvent, cx| {
                let MapEvent::Jump(row) = *event;
                window.jump_file_row(row, cx);
            });
            MapMount {
                entity,
                _jumps,
                layout,
            }
        });
        mount.entity.update(cx, |map, cx| {
            if mount.layout != layout {
                map.set_hunks(hunks, total, cx);
            }
            map.set_viewport(viewport, cx);
            map.set_current(current, cx);
        });
        mount.layout = layout;
        mount.entity.clone().into_any_element()
    }
}

impl ScreenOwner {
    /// Scrolls the open file diff so `row` (a pair row, as the map counts them) is in the middle
    /// of the list, which is asked in list items.
    fn jump_file_row(&mut self, row: u32, cx: &mut Context<Self>) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(view) = file.and_then(|file| file.view.as_mut()) {
            let item = view.list_row(row as usize);
            view.scroll.scroll_to_center(item);
            cx.notify();
        }
    }
}

#[cfg(test)]
impl FileView {
    /// The mounted map, for tests.
    pub(in crate::window_view::compare_screen) fn map_entity(&self) -> Option<Entity<ChangeMap>> {
        self.map.as_ref().map(|mount| mount.entity.clone())
    }
}

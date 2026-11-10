//! Which cards the Stream keeps (D26): the app hears the items in view plus a margin to build
//! first, and the UI drops cards more than 200 items from the view, on a scroll or on arrival.

use super::{FOCUS_MARGIN, KEEP_MARGIN, MIN_ROW, StreamScreen, item_of};
use crate::compare_diff::StreamUpdate;
use crate::prelude::*;

impl StreamScreen {
    /// The first and last positions in `visible` that the viewport shows, estimated from the
    /// list's top row and the shortest row height; `None` before any row is listed.
    fn in_view(&self, visible: &[usize]) -> Option<(usize, usize)> {
        let top = self.list.logical_scroll_top().item_ix;
        let height = self.list.viewport_bounds().size.height;
        // Rounded up: a partly shown row counts as in view.
        let fit = (height / px(MIN_ROW)).ceil().max(0.) as usize;
        let end = (top + fit + 1).min(self.rows.len());
        let shown = self.rows.get(top.min(end)..end)?.iter().filter_map(item_of);
        let (low, high) = (shown.clone().min()?, shown.max()?);
        let position = |item: usize| visible.partition_point(|&at| at < item);
        Some((position(low), position(high)))
    }

    /// Tells the app which cards to build first (the items in view plus 10 on each side) and
    /// drops cards more than 200 items away. Returns whether rows changed.
    pub(super) fn send_focus(&mut self) -> bool {
        let visible = self.model.visible();
        let Some(last) = visible.len().checked_sub(1) else {
            return false;
        };
        let (low, high) = self.in_view(&visible).unwrap_or((0, 0));
        if self.sent != Some((low, high)) {
            self.sent = Some((low, high));
            let near = low.saturating_sub(FOCUS_MARGIN)..=(high + FOCUS_MARGIN).min(last);
            let items = self.model.items();
            let ids = visible[near].iter().map(|&at| items[at].id).collect();
            self.control.focus(ids);
        }
        let dropped = self.evict_far(&visible, (low, high));
        if dropped {
            self.sync_rows();
        }
        dropped
    }

    /// Drops the cards that arrived or stayed more than 200 items from the view, without
    /// rebuilding the rows. Returns whether any dropped.
    pub(super) fn drop_far_cards(&mut self) -> bool {
        let visible = self.model.visible();
        let view = self.in_view(&visible).unwrap_or((0, 0));
        self.evict_far(&visible, view)
    }

    /// Evicts the cards of items more than 200 positions from the view `(low, high)`; their
    /// rows become placeholders until the app sends them again. Returns whether any dropped.
    fn evict_far(&mut self, visible: &[usize], (low, high): (usize, usize)) -> bool {
        let items = self.model.items();
        let far: Vec<u64> = visible
            .iter()
            .enumerate()
            .filter(|&(at, _)| at + KEEP_MARGIN < low || at > high + KEEP_MARGIN)
            .map(|(_, &item)| items[item].id)
            .filter(|&id| self.model.card(id).is_some())
            .collect();
        let generation = self.model.generation();
        for &id in &far {
            self.model.apply(&StreamUpdate::Evicted { generation, id });
        }
        !far.is_empty()
    }
}

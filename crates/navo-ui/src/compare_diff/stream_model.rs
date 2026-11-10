//! Pure state of the Stream screen: its items, cards, cursor, Viewed and fold marks, search narrowing.
//! No gpui here; the host draws from this model and feeds it `StreamUpdate`s (D25, D27).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

mod items;
mod update;

use super::hooks::{Card, ViewedKey};

pub use items::{EscStep, Stop, StreamItem, StreamList, is_stream_item, stream_items};

/// The Stream screen's state.
#[derive(Debug, Default)]
pub struct StreamModel {
    pub(super) items: Vec<StreamItem>,
    pub(super) rules: usize,
    pub(super) generation: u64,
    pub(super) cards: HashMap<u64, Arc<Card>>,
    pub(super) viewed: HashSet<u64>,
    /// Items whose fold differs from the default: a viewed card is folded, others are open.
    pub(super) toggled: HashSet<u64>,
    pub(super) cursor: Stop,
    pub(super) query: u64,
    /// Item id to the hunk indexes that match; `None` when no search narrows the Stream.
    pub(super) hits: Option<HashMap<u64, Vec<u32>>>,
    pub(super) outline_open: bool,
    pub(super) progress: (u32, u32),
}

impl StreamModel {
    /// The listed items in path order.
    #[must_use]
    pub fn items(&self) -> &[StreamItem] {
        &self.items
    }

    /// How many same-after-rules files form the folded group at the end.
    #[must_use]
    pub fn rules(&self) -> usize {
        self.rules
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn cursor(&self) -> Stop {
        self.cursor
    }

    /// The built card of item `id`, if the app sent one and has not evicted it.
    #[must_use]
    pub fn card(&self, id: u64) -> Option<&Arc<Card>> {
        self.cards.get(&id)
    }

    #[must_use]
    pub fn is_viewed(&self, id: u64) -> bool {
        self.viewed.contains(&id)
    }

    /// Whether item `id` shows its changes: not viewed and not folded, or opened with O.
    #[must_use]
    pub fn is_open(&self, id: u64) -> bool {
        self.viewed.contains(&id) == self.toggled.contains(&id)
    }

    /// Cards built so far and the total, as the app last reported.
    #[must_use]
    pub fn progress(&self) -> (u32, u32) {
        self.progress
    }

    /// Whether a search narrows the Stream now.
    #[must_use]
    pub fn searching(&self) -> bool {
        self.hits.is_some()
    }

    #[must_use]
    pub fn outline_open(&self) -> bool {
        self.outline_open
    }

    /// Opens or closes the outline drawer (Ctrl+B); Esc closes it after the search.
    pub fn set_outline(&mut self, open: bool) {
        self.outline_open = open;
    }

    /// How many changes of item `id` the search matched; `None` when no search narrows the
    /// Stream, zero for a path-only match.
    #[must_use]
    pub fn hit_count(&self, id: u64) -> Option<usize> {
        self.hits.as_ref()?.get(&id).map(Vec::len)
    }

    /// Puts the cursor on the first stop of item `item` (an outline jump). Returns whether the
    /// item is shown.
    pub fn jump_to(&mut self, item: usize) -> bool {
        match self.stops().into_iter().find(|stop| stop.item == item) {
            Some(stop) => {
                self.cursor = stop;
                true
            }
            None => false,
        }
    }

    /// Item indexes shown now: all of them, or the ones the search matched.
    #[must_use]
    pub fn visible(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|&at| {
                self.hits
                    .as_ref()
                    .is_none_or(|h| h.contains_key(&self.items[at].id))
            })
            .collect()
    }

    /// Every J/K stop in order. A folded, viewed, summary or placeholder card is one stop.
    #[must_use]
    pub fn stops(&self) -> Vec<Stop> {
        let mut stops = Vec::new();
        for at in self.visible() {
            let id = self.items[at].id;
            let hunks = self
                .cards
                .get(&id)
                .filter(|_| self.is_open(id))
                .map_or(0, |card| {
                    u32::try_from(card.hunks.len()).unwrap_or(u32::MAX)
                });
            // A path-only match has no hunk hits; all its hunks stay stops.
            let matched = self
                .hits
                .as_ref()
                .and_then(|h| h.get(&id))
                .filter(|m| !m.is_empty());
            let whole = Stop {
                item: at,
                hunk: None,
            };
            let hunk = |h: u32| Stop {
                item: at,
                hunk: Some(h),
            };
            match (hunks, matched) {
                (0, _) => stops.push(whole),
                (_, Some(list)) => {
                    stops.extend(list.iter().filter(|&&h| h < hunks).map(|&h| hunk(h)))
                }
                (count, None) => stops.extend((0..count).map(hunk)),
            }
        }
        stops
    }

    /// J (`1`) or K (`-1`): moves to the next or previous change. Returns whether it moved.
    pub fn next_change(&mut self, step: i32) -> bool {
        let stops = self.stops();
        let here = stops.iter().position(|s| *s == self.cursor);
        let target = match (here, step > 0) {
            (Some(at), true) => at.checked_add(1),
            (Some(at), false) => at.checked_sub(1),
            (None, true) => stops.iter().position(|s| s.item >= self.cursor.item),
            (None, false) => stops.iter().rposition(|s| s.item <= self.cursor.item),
        };
        self.go(&stops, target)
    }

    /// Shift+J (`1`) or Shift+K (`-1`): moves to the first stop of the next or previous file.
    pub fn next_file(&mut self, step: i32) -> bool {
        let stops = self.stops();
        let item = self.cursor.item;
        let target = if step > 0 {
            stops.iter().position(|s| s.item > item)
        } else {
            stops.iter().rposition(|s| s.item < item).and_then(|last| {
                let prev = stops[last].item;
                stops.iter().position(|s| s.item == prev)
            })
        };
        self.go(&stops, target)
    }

    /// Moves the cursor to `stops[target]` when it exists; returns whether it moved.
    fn go(&mut self, stops: &[Stop], target: Option<usize>) -> bool {
        match target.and_then(|at| stops.get(at)) {
            Some(stop) => {
                self.cursor = *stop;
                true
            }
            None => false,
        }
    }

    /// V: flips the Viewed mark of the current file. Setting it folds the card and moves on.
    /// Returns the item id, its key and the new state; `None` while the card is not built or
    /// its content could not be hashed.
    pub fn toggle_viewed(&mut self) -> Option<(u64, ViewedKey, bool)> {
        let id = self.items.get(self.cursor.item)?.id;
        let card = self.cards.get(&id).filter(|card| card.viewable)?;
        let key = card.viewed_key;
        let now = self.viewed.insert(id);
        if !now {
            self.viewed.remove(&id);
        }
        self.toggled.remove(&id);
        self.cursor.hunk = None;
        if !(now && self.next_file(1)) {
            self.fix_cursor();
        }
        Some((id, key, now))
    }

    /// O: folds or opens the current card. A viewed card opens without losing its mark.
    pub fn toggle_fold(&mut self) {
        let Some(id) = self.items.get(self.cursor.item).map(|i| i.id) else {
            return;
        };
        if !self.toggled.remove(&id) {
            self.toggled.insert(id);
        }
        if !self.is_open(id) {
            self.cursor.hunk = None;
        }
        self.fix_cursor();
    }

    /// "Viewed n of N": N counts the items without the same-after-rules group.
    #[must_use]
    pub fn counter(&self) -> (usize, usize) {
        let viewed = self
            .items
            .iter()
            .filter(|i| self.viewed.contains(&i.id))
            .count();
        (viewed, self.items.len())
    }

    /// Puts the cursor on a real stop when its hunk or item is gone: the first stop at or after
    /// its item, else the last stop.
    pub(super) fn fix_cursor(&mut self) {
        let stops = self.stops();
        let Some(last) = stops.last() else {
            return;
        };
        if stops.contains(&self.cursor) {
            return;
        }
        let item = self.cursor.item;
        self.cursor = *stops.iter().find(|s| s.item >= item).unwrap_or(last);
    }
}

#[cfg(test)]
mod tests;

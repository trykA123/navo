//! How the Stream model takes worker updates, new item lists, a search and Esc (D24, D25, D30).

use std::collections::{HashMap, HashSet};

use super::{EscStep, Stop, StreamList, StreamModel};
use crate::compare_diff::hooks::{Card, StreamUpdate};

impl StreamModel {
    /// Takes a worker update. One tagged with another generation, or an old query, is dropped.
    pub fn apply(&mut self, update: &StreamUpdate) {
        match update {
            StreamUpdate::Card {
                generation,
                id,
                card,
            } if *generation == self.generation => {
                if self.items.iter().any(|i| i.id == *id) {
                    self.take_viewed(*id, card);
                    self.cards.insert(*id, card.clone());
                }
            }
            StreamUpdate::Evicted { generation, id } if *generation == self.generation => {
                self.cards.remove(id);
            }
            StreamUpdate::Viewed { id, viewed } => self.mark_viewed(*id, *viewed),
            StreamUpdate::Progress {
                generation,
                built,
                total,
            } if *generation == self.generation => {
                self.progress = (*built, *total);
            }
            StreamUpdate::Hits {
                generation,
                query,
                hits,
                ..
            } if *generation == self.generation && *query == self.query => {
                // Hits may come in parts; a later part for the same file replaces the earlier one.
                if let Some(found) = self.hits.as_mut() {
                    found.extend(hits.iter().cloned());
                }
            }
            _ => {}
        }
        self.fix_cursor();
    }

    /// Takes the Viewed mark of a new card, unless the same content is already on screen:
    /// then the mark set here in the meantime wins. New content shows its stored mark (D28).
    fn take_viewed(&mut self, id: u64, card: &Card) {
        let same = self
            .cards
            .get(&id)
            .is_some_and(|old| old.viewed_key == card.viewed_key);
        if !same {
            self.mark_viewed(id, card.viewed);
        }
    }

    /// Sets the Viewed mark of `id`; a change puts its fold back to the default.
    fn mark_viewed(&mut self, id: u64, viewed: bool) {
        let changed = if viewed {
            self.viewed.insert(id)
        } else {
            self.viewed.remove(&id)
        };
        if changed {
            self.toggled.remove(&id);
        }
    }

    /// Replaces the items for a new `generation`; the cursor stays on the same item id.
    /// Cards of items still listed stay until the app sends new ones.
    pub fn set_items(&mut self, generation: u64, list: StreamList) {
        let kept = self.items.get(self.cursor.item).map(|i| i.id);
        self.generation = generation;
        self.items = list.items;
        self.rules = list.rules;
        let ids: HashSet<u64> = self.items.iter().map(|i| i.id).collect();
        self.cards.retain(|id, _| ids.contains(id));
        self.cursor = match kept.and_then(|id| self.items.iter().position(|i| i.id == id)) {
            Some(item) => Stop {
                item,
                hunk: self.cursor.hunk,
            },
            None => Stop {
                item: self.cursor.item.min(self.items.len().saturating_sub(1)),
                hunk: None,
            },
        };
        self.fix_cursor();
    }

    /// Starts narrowing to search `query`, a new tag per typed text; hits arrive as
    /// `StreamUpdate::Hits`. Until they do, nothing matches.
    pub fn begin_search(&mut self, query: u64) {
        self.query = query;
        self.hits = Some(HashMap::new());
        self.fix_cursor();
    }

    /// Esc: closes the search, then the outline, then leaves the Stream.
    pub fn esc(&mut self) -> EscStep {
        if self.hits.take().is_some() {
            self.fix_cursor();
            EscStep::ClearSearch
        } else if std::mem::take(&mut self.outline_open) {
            EscStep::CloseOutline
        } else {
            EscStep::Leave
        }
    }
}

#[cfg(test)]
mod tests;

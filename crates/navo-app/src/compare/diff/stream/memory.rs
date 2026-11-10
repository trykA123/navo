//! Card-body accounting and counter conversion shared by Stream state and search.

use std::collections::HashMap;
use std::mem::size_of;

use navo_ui::compare_diff::{Card, CardRow, HunkView, PairRequest};

use super::state::State;

/// Fixed bytes per card, so cards without rows (summary cards) still count against the budget.
pub(crate) const CARD_BYTES: u64 = 256;

/// Card body bytes: 256 per card, the hunk list, and row text plus 64 bytes per row (D26).
pub(crate) fn card_bytes(card: &Card) -> u64 {
    let hunks = card.hunks.len().saturating_mul(size_of::<HunkView>());
    let fixed = CARD_BYTES.saturating_add(u64::try_from(hunks).unwrap_or(u64::MAX));
    card.rows
        .iter()
        .map(|row| match row {
            CardRow::Line { left, right, .. } => {
                let text = left
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(right.as_ref().map_or(0, String::len));
                64_u64 + u64::try_from(text).unwrap_or(u64::MAX)
            }
            CardRow::Gap { .. } | CardRow::More { .. } => 64,
        })
        .fold(fixed, u64::saturating_add)
}

/// Saturating conversion to the UI's counters.
pub(super) fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl State {
    /// The held card with the oldest focus age, then the earliest item order.
    pub(super) fn victim(&self) -> Option<u64> {
        self.held
            .keys()
            .min_by_key(|id| {
                (
                    self.focus_age.get(*id).copied().unwrap_or(0),
                    self.positions.get(*id).copied().unwrap_or(usize::MAX),
                    **id,
                )
            })
            .copied()
    }

    /// Drops the held card of `id` and queues it for a new build, so a copy that changed its
    /// files shows at once; other cards stay.
    pub(super) fn invalidate(&mut self, id: u64) {
        if let Some(card) = self.held.remove(&id) {
            self.held_bytes = self.held_bytes.saturating_sub(card_bytes(&card));
        }
        self.built.remove(&id);
        self.enqueue(id);
    }

    /// Drops held cards whose item left the list or whose pair changed, within one generation.
    pub(super) fn drop_stale(&mut self, previous: &HashMap<u64, PairRequest>) {
        let stale: Vec<u64> = self
            .held
            .keys()
            .filter(|id| {
                self.requests
                    .get(*id)
                    .is_none_or(|now| previous.get(*id) != Some(now))
            })
            .copied()
            .collect();
        for id in stale {
            if let Some(card) = self.held.remove(&id) {
                self.held_bytes = self.held_bytes.saturating_sub(card_bytes(&card));
            }
            self.built.remove(&id);
        }
    }
}

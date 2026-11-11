//! Which card detail runs next, which ones the session holds, and their share of the card budget (D26.1, D26.3).

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::sync::Arc;

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{CardDetail, LargeHunk, PairKind, PairRequest};

use super::memory::{CARD_BYTES, card_bytes};
use super::state::State;

#[cfg(test)]
mod tests;

/// A detail job taken for one heavy held card; `cancel` stops it when the card leaves.
pub(crate) struct DetailJob {
    pub id: u64,
    pub generation: u64,
    pub request: PairRequest,
    pub kind: PairKind,
    pub cancel: CancelToken,
}

/// A stored detail snapshot, with the ids its bytes pushed out of the card budget.
pub(crate) struct DetailFinish {
    pub generation: u64,
    pub id: u64,
    pub detail: Arc<CardDetail>,
    pub evicted: Vec<u64>,
}

/// One held detail: its body, whether it is final, and the bytes it adds to the budget.
struct Held {
    detail: Arc<CardDetail>,
    last: bool,
    bytes: u64,
}

/// Details held for focused ids, the single running job, and the ids tried once.
#[derive(Default)]
pub(crate) struct Details {
    held: HashMap<u64, Held>,
    bytes: u64,
    order: Vec<u64>,
    running: Option<(u64, CancelToken)>,
    tried: HashSet<u64>,
}

impl Details {
    /// Bytes the held details add to the card budget.
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Remember the focus order and cancel a running job whose id left it.
    pub(super) fn set_order(&mut self, ids: &[u64]) {
        self.order = ids.to_vec();
        if let Some((id, token)) = &self.running
            && !ids.contains(id)
        {
            token.cancel();
        }
    }

    /// Drop one id's detail and retry mark, cancelling the job when it is that id.
    pub(super) fn forget(&mut self, id: u64) {
        if let Some(held) = self.held.remove(&id) {
            self.bytes = self.bytes.saturating_sub(held.bytes);
        }
        self.tried.remove(&id);
        if let Some((running, token)) = &self.running
            && *running == id
        {
            token.cancel();
        }
    }

    /// Cancel any running job and drop every detail, keeping the focus order.
    ///
    /// The cancelled job keeps the single lane until `end_detail`, so two engines never overlap.
    pub(super) fn clear(&mut self) {
        self.cancel_running();
        self.held.clear();
        self.bytes = 0;
        self.tried.clear();
    }

    /// Cancel the running job's token, if there is one.
    pub(super) fn cancel_running(&self) {
        if let Some((_, token)) = &self.running {
            token.cancel();
        }
    }
}

/// Bytes a detail adds to the card budget: its fixed card share plus its bodies (D26.3).
pub(crate) fn detail_bytes(detail: &CardDetail) -> u64 {
    let mut bytes = CARD_BYTES;
    match detail {
        CardDetail::Large(card) => {
            let hunks = card.hunks.len().saturating_mul(size_of::<LargeHunk>());
            bytes = bytes.saturating_add(u64::try_from(hunks).unwrap_or(u64::MAX));
        }
        CardDetail::Bytes(card) => {
            if let Some(window) = &card.window {
                let window_bytes = window.left.len().saturating_add(window.right.len());
                bytes = bytes.saturating_add(u64::try_from(window_bytes).unwrap_or(u64::MAX));
            }
        }
        CardDetail::Image(card) => {
            for thumb in [&card.left, &card.right].into_iter().flatten() {
                bytes = bytes.saturating_add(u64::try_from(thumb.bgra.len()).unwrap_or(u64::MAX));
            }
        }
        CardDetail::Failed(text) => {
            bytes = bytes.saturating_add(u64::try_from(text.len()).unwrap_or(u64::MAX));
        }
    }
    bytes
}

impl State {
    /// Take the heavy card nearest the focus middle, if the lane is free (D26.1).
    ///
    /// Returns `None` while a job runs or no focused card needs a detail.
    pub(crate) fn take_detail(&mut self) -> Option<DetailJob> {
        if self.details.running.is_some() {
            return None;
        }
        let len = self.details.order.len();
        let pick = self
            .details
            .order
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, id)| self.detail_candidate(*id))
            .map(|(index, id)| {
                (
                    index.saturating_mul(2).abs_diff(len.saturating_sub(1)),
                    index,
                    id,
                )
            })
            .min_by_key(|(distance, index, _)| (*distance, *index))?;
        let (_, _, id) = pick;
        let request = self.requests.get(&id)?.clone();
        let kind = self.held.get(&id)?.kind;
        let token = CancelToken::new();
        self.details.running = Some((id, token.clone()));
        Some(DetailJob {
            id,
            generation: self.generation(),
            request,
            kind,
            cancel: token,
        })
    }

    /// Store a detail snapshot for the running job and evict cards until the budget holds.
    ///
    /// Returns `None` for a stale, cancelled or non-running job, or one without a held card.
    pub(crate) fn store_detail(
        &mut self,
        job: &DetailJob,
        detail: Arc<CardDetail>,
        last: bool,
    ) -> Option<DetailFinish> {
        let running = self.details.running.as_ref().map(|(id, _)| *id);
        if job.generation != self.generation()
            || job.cancel.is_cancelled()
            || running != Some(job.id)
            || !self.held.contains_key(&job.id)
        {
            return None;
        }
        let bytes = detail_bytes(&detail);
        let old = self.details.held.insert(
            job.id,
            Held {
                detail: Arc::clone(&detail),
                last,
                bytes,
            },
        );
        if let Some(old) = old {
            self.details.bytes = self.details.bytes.saturating_sub(old.bytes);
        }
        self.details.bytes = self.details.bytes.saturating_add(bytes);
        let evicted = self.evict_over_budget();
        Some(DetailFinish {
            generation: job.generation,
            id: job.id,
            detail,
            evicted,
        })
    }

    /// End a detail job: free the lane and remember an unfinished id so it is not retried.
    pub(crate) fn end_detail(&mut self, job: &DetailJob) {
        if self
            .details
            .running
            .as_ref()
            .is_some_and(|(id, _)| *id == job.id)
        {
            self.details.running = None;
        }
        let last = self.details.held.get(&job.id).is_some_and(|held| held.last);
        if !job.cancel.is_cancelled() && !last {
            self.details.tried.insert(job.id);
        }
    }

    /// The held details of the focused ids in focus order, for resending after a focus.
    pub(crate) fn focused_details(&self) -> Vec<(u64, Arc<CardDetail>)> {
        self.details
            .order
            .iter()
            .copied()
            .filter(|id| self.held.contains_key(id))
            .filter_map(|id| {
                self.details
                    .held
                    .get(&id)
                    .map(|held| (id, Arc::clone(&held.detail)))
            })
            .collect()
    }

    /// Evict least recently focused cards until cards and details fit the budget (D26.3).
    pub(super) fn evict_over_budget(&mut self) -> Vec<u64> {
        let mut evicted = Vec::new();
        while self.held_bytes.saturating_add(self.details.bytes()) > self.budget {
            let Some(victim) = self.victim() else {
                break;
            };
            let Some(removed) = self.held.remove(&victim) else {
                break;
            };
            self.held_bytes = self.held_bytes.saturating_sub(card_bytes(&removed));
            self.details.forget(victim);
            evicted.push(victim);
        }
        evicted
    }

    /// True when `id` holds a heavy card that still needs a detail and was not tried.
    fn detail_candidate(&self, id: u64) -> bool {
        let Some(card) = self.held.get(&id) else {
            return false;
        };
        if !matches!(
            card.kind,
            PairKind::Large { .. } | PairKind::Binary | PairKind::Image
        ) {
            return false;
        }
        if self.details.tried.contains(&id) || !self.requests.contains_key(&id) {
            return false;
        }
        self.details.held.get(&id).is_none_or(|held| !held.last)
    }
}

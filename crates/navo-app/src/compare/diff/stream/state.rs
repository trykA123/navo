//! Queue and memory state for one Stream generation.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{Card, PairRequest};

use super::detail_state::Details;
use super::memory::{card_bytes, count};
use super::search::{SearchJob, SearchState};

#[cfg(test)]
mod probe;

/// A card ready to send, with the bodies it pushed out and the progress counters.
pub(crate) struct Finish {
    pub generation: u64,
    pub id: u64,
    pub card: Arc<Card>,
    pub evicted: Vec<u64>,
    pub built: u32,
    pub total: u32,
}

/// A card job taken off the queue; `cancel` belongs to the generation that queued it.
pub(crate) struct CardJob {
    pub id: u64,
    pub request: PairRequest,
    pub generation: u64,
    pub cancel: CancelToken,
}

/// One queued Viewed write; it carries no path and is independent of the generation.
pub(crate) struct ViewedWrite {
    pub id: u64,
    pub key: [u8; 64],
    pub viewed: bool,
}

/// Work for a Stream worker: build a card, store a Viewed mark, or run a search.
pub(super) enum Job {
    Card(CardJob),
    Viewed(ViewedWrite),
    Search(SearchJob),
}

/// What one generation holds: settled items, priority queues, focus age, card bodies and search.
pub(crate) struct State {
    generation: u64,
    pub(super) requests: HashMap<u64, PairRequest>,
    pub(super) positions: HashMap<u64, usize>,
    focus: HashSet<u64>,
    pub(super) focus_age: HashMap<u64, u64>,
    focus_clock: u64,
    focus_queue: VecDeque<u64>,
    /// Ids waiting in `focus_queue`, so a repeated focus does not queue an id twice.
    focus_queued: HashSet<u64>,
    normal_queue: VecDeque<u64>,
    viewed_queue: VecDeque<ViewedWrite>,
    scheduled: HashSet<u64>,
    in_progress: HashSet<u64>,
    pub(super) held: HashMap<u64, Arc<Card>>,
    pub(super) details: Details,
    pub(super) held_bytes: u64,
    pub(super) built: HashSet<u64>,
    pub(super) budget: u64,
    generation_cancel: CancelToken,
    pub(super) search: SearchState,
}

impl State {
    /// Start empty at generation zero with the effective card budget.
    pub(crate) fn new(budget: u64) -> Self {
        Self {
            generation: 0,
            requests: HashMap::new(),
            positions: HashMap::new(),
            focus: HashSet::new(),
            focus_age: HashMap::new(),
            focus_clock: 0,
            focus_queue: VecDeque::new(),
            focus_queued: HashSet::new(),
            normal_queue: VecDeque::new(),
            viewed_queue: VecDeque::new(),
            scheduled: HashSet::new(),
            in_progress: HashSet::new(),
            held: HashMap::new(),
            details: Details::default(),
            held_bytes: 0,
            built: HashSet::new(),
            budget,
            generation_cancel: CancelToken::new(),
            search: SearchState::default(),
        }
    }

    /// Replace the items. A new generation drops old cards, queues and loads.
    pub(crate) fn set_items(&mut self, generation: u64, items: Vec<(PairRequest, bool)>) {
        if generation != self.generation {
            self.generation = generation;
            self.search.cancel();
            self.generation_cancel.cancel();
            self.generation_cancel = CancelToken::new();
            self.held.clear();
            self.held_bytes = 0;
            self.details.clear();
            self.built.clear();
            self.in_progress.clear();
        }
        let previous = std::mem::take(&mut self.requests);
        self.positions.clear();
        self.focus_queue.clear();
        self.focus_queued.clear();
        self.normal_queue.clear();
        self.scheduled.clear();
        let mut ids = Vec::with_capacity(items.len());
        for (position, (request, settled)) in items.into_iter().enumerate() {
            if !settled {
                continue;
            }
            ids.push(request.id);
            self.positions.insert(request.id, position);
            self.requests.insert(request.id, request);
        }
        self.drop_stale(&previous);
        for id in ids {
            self.enqueue(id);
        }
    }

    /// Replace the focused set with `ids`, move queued ones first, and return held cards.
    ///
    /// The returned cards are sent again because the UI may have dropped a body that is too far
    /// from the view. A duplicate id is returned once.
    pub(crate) fn focus(&mut self, ids: Vec<u64>) -> Vec<(u64, Arc<Card>)> {
        self.details.set_order(&ids);
        self.focus = ids.iter().copied().collect();
        let mut held = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            self.focus_clock = self.focus_clock.saturating_add(1);
            let age = self.focus_clock;
            self.focus_age.insert(id, age);
            if let Some(card) = self.held.get(&id)
                && self.requests.contains_key(&id)
                && seen.insert(id)
            {
                held.push((id, Arc::clone(card)));
            }
            self.reprioritise(id);
        }
        held
    }

    /// Current generation, for resending a held card.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Queue one Viewed write; the caller never touches the store itself.
    pub(super) fn set_viewed(&mut self, write: ViewedWrite) {
        self.viewed_queue.push_back(write);
    }

    /// Take the next Viewed write or highest-priority card job.
    pub(super) fn take_job(&mut self) -> Option<Job> {
        if let Some(job) = self.search.take_job() {
            return Some(Job::Search(job));
        }
        if let Some(write) = self.viewed_queue.pop_front() {
            return Some(Job::Viewed(write));
        }
        loop {
            let id = match self.focus_queue.pop_front() {
                Some(id) => {
                    self.focus_queued.remove(&id);
                    id
                }
                None => self.normal_queue.pop_front()?,
            };
            // A focused id leaves a stale copy in the normal queue; `scheduled` drops it here.
            if !self.scheduled.remove(&id)
                || self.held.contains_key(&id)
                || self.in_progress.contains(&id)
            {
                continue;
            }
            let Some(request) = self.requests.get(&id).cloned() else {
                continue;
            };
            self.in_progress.insert(id);
            return Some(Job::Card(CardJob {
                id,
                request,
                generation: self.generation,
                cancel: self.generation_cancel.clone(),
            }));
        }
    }

    /// Store a built card, evict least recently focused bodies, and report what changed.
    pub(super) fn finish(&mut self, id: u64, generation: u64, card: Arc<Card>) -> Option<Finish> {
        self.in_progress.remove(&id);
        if generation != self.generation || !self.requests.contains_key(&id) {
            return None;
        }
        if self.held.contains_key(&id) {
            return None;
        }
        self.held_bytes = self.held_bytes.saturating_add(card_bytes(&card));
        self.held.insert(id, Arc::clone(&card));
        self.built.insert(id);
        let evicted = self.evict_over_budget();
        Some(Finish {
            generation,
            id,
            card,
            evicted,
            built: count(self.built.len()),
            total: count(self.requests.len()),
        })
    }

    /// Forget an in-progress job whose load returned no card.
    pub(super) fn abandon(&mut self, id: u64) {
        self.in_progress.remove(&id);
    }

    /// Number of settled items in this generation.
    pub(super) fn total(&self) -> u32 {
        count(self.requests.len())
    }

    /// Number of distinct items that have produced a card in this generation.
    pub(super) fn built(&self) -> u32 {
        count(self.built.len())
    }

    /// True when some queue still has an item or write to try.
    pub(super) fn has_work(&self) -> bool {
        !self.focus_queue.is_empty()
            || !self.normal_queue.is_empty()
            || !self.viewed_queue.is_empty()
            || self.search.has_work()
    }

    /// Stop all loads and any search in this generation.
    pub(super) fn cancel(&mut self) {
        self.generation_cancel.cancel();
        self.details.cancel_running();
        self.search.cancel();
    }

    /// Put an id in the matching priority queue once.
    pub(super) fn enqueue(&mut self, id: u64) {
        if self.held.contains_key(&id)
            || self.in_progress.contains(&id)
            || self.scheduled.contains(&id)
            || !self.requests.contains_key(&id)
        {
            return;
        }
        self.scheduled.insert(id);
        if self.focus.contains(&id) {
            self.push_focus(id);
        } else {
            self.normal_queue.push_back(id);
        }
    }

    /// Put a scheduled item in the focus queue in O(1); its normal copy becomes stale.
    fn reprioritise(&mut self, id: u64) {
        if self.scheduled.contains(&id) {
            self.push_focus(id);
        } else {
            self.enqueue(id);
        }
    }

    /// Append to the focus queue unless the id already waits there.
    fn push_focus(&mut self, id: u64) {
        if self.focus_queued.insert(id) {
            self.focus_queue.push_back(id);
        }
    }
}

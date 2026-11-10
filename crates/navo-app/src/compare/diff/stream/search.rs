//! Search across the Stream: held cards first, then the missing ones (D30, V28).
//!
//! One worker runs a query, sends partial `Hits` every 100 ms and a final one. A newer query
//! cancels the older; the built cards still count against the card budget.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{Card, CardRow, DiffRow, PairRequest, StreamUpdate};

use super::Shared;
use super::worker;

/// How often a running search sends its partial hits.
const HITS_EVERY: Duration = Duration::from_millis(100);

/// One query a worker runs; its token cancels the query when a newer one arrives.
pub(super) struct SearchJob {
    /// A local sequence number, so an older worker cannot finish a newer search.
    pub id: u64,
    pub query: u64,
    pub generation: u64,
    /// The query in lower case, so every comparison is case-insensitive.
    pub text: String,
    pub cancel: CancelToken,
}

/// The newest query and the one a worker is running now.
#[derive(Default)]
pub(super) struct SearchState {
    current: Option<SearchJob>,
    running: Option<(u64, CancelToken)>,
    next_id: u64,
}

impl SearchState {
    /// Queue a new query, cancel the running one, and drop any older waiting query.
    pub(super) fn request(&mut self, query: u64, generation: u64, text: String) {
        self.cancel();
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.current = Some(SearchJob {
            id,
            query,
            generation,
            text: text.to_lowercase(),
            cancel: CancelToken::new(),
        });
    }

    /// Stop the running query and forget every waiting one.
    pub(super) fn cancel(&mut self) {
        if let Some((_, running)) = self.running.take() {
            running.cancel();
        }
        self.current = None;
    }

    /// Take the newest query when no other one runs.
    pub(super) fn take_job(&mut self) -> Option<SearchJob> {
        if self.running.is_some() {
            return None;
        }
        let job = self.current.take()?;
        self.running = Some((job.id, job.cancel.clone()));
        Some(job)
    }

    /// Mark the running query finished; an older worker never clears a newer one.
    pub(super) fn finish(&mut self, id: u64) {
        if self
            .running
            .as_ref()
            .is_some_and(|(running, _)| *running == id)
        {
            self.running = None;
        }
    }

    /// True when a worker can take a query now.
    pub(super) fn has_work(&self) -> bool {
        self.running.is_none() && self.current.is_some()
    }
}

/// Counters and hits of one running query.
struct Scan {
    text: String,
    hits: Vec<(u64, Vec<u32>)>,
    /// How many of `hits` earlier messages already carried.
    sent: usize,
    searched: u32,
    total: u32,
    last: Instant,
}

impl Scan {
    /// Start a scan over `total` settled items with the lower-case query.
    fn new(total: u32, text: String) -> Self {
        Self {
            text,
            hits: Vec::new(),
            sent: 0,
            searched: 0,
            total,
            last: Instant::now(),
        }
    }

    /// Add one item's matching hunks; a path-only match has an empty list.
    fn consider(&mut self, id: u64, path: &str, card: &Card) {
        if let Some(hunks) = matching_hunks(&self.text, path, card) {
            self.hits.push((id, hunks));
        }
    }

    /// Send partial hits when 100 ms passed, or the final hits when `done`.
    fn tick(&mut self, shared: &Arc<Shared>, job: &SearchJob, done: bool) {
        if done || self.last.elapsed() >= HITS_EVERY {
            self.send(shared, job, done);
            self.last = Instant::now();
        }
    }

    /// Send the hits found since the last message of this query; the model adds them up.
    fn send(&mut self, shared: &Arc<Shared>, job: &SearchJob, done: bool) {
        let fresh = self.hits[self.sent..].to_vec();
        self.sent = self.hits.len();
        (shared.sink)(StreamUpdate::Hits {
            generation: job.generation,
            query: job.query,
            hits: fresh,
            searched: self.searched,
            total: self.total,
            done,
        });
    }
}

/// Run one query on this worker and mark the search state finished.
pub(super) fn run(shared: &Arc<Shared>, job: SearchJob) {
    let (total, items) = snapshot(shared);
    let mut scan = Scan::new(total, job.text.clone());
    if !scan_all(shared, &job, items, &mut scan) && !stopped(shared, &job) {
        scan.tick(shared, &job, true);
    }
    let mut state = shared.lock_state();
    state.search.finish(job.id);
    drop(state);
    shared.ready.notify_all();
}

/// Scan held cards first, then build and scan the missing items; true when cancelled.
fn scan_all(
    shared: &Arc<Shared>,
    job: &SearchJob,
    items: Vec<(u64, PairRequest)>,
    scan: &mut Scan,
) -> bool {
    let mut missing = Vec::new();
    for (id, request) in items {
        if stopped(shared, job) {
            return true;
        }
        match held(shared, id) {
            Some(card) => {
                scan.consider(id, &request.path, &card);
                scan.searched = scan.searched.saturating_add(1);
                scan.tick(shared, job, false);
            }
            None => missing.push((id, request)),
        }
    }
    for (id, request) in missing {
        if stopped(shared, job) {
            return true;
        }
        match build(shared, &request, job) {
            Some(card) => scan.consider(id, &request.path, &card),
            None if path_matches(&scan.text, &request.path) => {
                scan.hits.push((id, Vec::new()));
            }
            None => {}
        }
        if stopped(shared, job) {
            return true;
        }
        scan.searched = scan.searched.saturating_add(1);
        scan.tick(shared, job, false);
    }
    false
}

/// The settled items in path order, with the total count.
fn snapshot(shared: &Arc<Shared>) -> (u32, Vec<(u64, PairRequest)>) {
    let state = shared.lock_state();
    let mut items: Vec<(usize, u64, PairRequest)> = state
        .requests
        .iter()
        .map(|(id, request)| {
            (
                state.positions.get(id).copied().unwrap_or(usize::MAX),
                *id,
                request.clone(),
            )
        })
        .collect();
    items.sort_by_key(|(position, _, _)| *position);
    (
        state.total(),
        items
            .into_iter()
            .map(|(_, id, request)| (id, request))
            .collect(),
    )
}

/// The held card of an item, if this generation built one.
fn held(shared: &Arc<Shared>, id: u64) -> Option<Arc<Card>> {
    shared.lock_state().held.get(&id).cloned()
}

/// Build a missing card, store it in the budget and publish it.
fn build(shared: &Arc<Shared>, request: &PairRequest, job: &SearchJob) -> Option<Arc<Card>> {
    let card = worker::build_card(shared, request, &job.cancel)?;
    if job.cancel.is_cancelled() {
        return None;
    }
    shared.store_and_publish(request.id, job.generation, Arc::clone(&card));
    Some(card)
}

/// True when this query was cancelled or the session is stopping.
fn stopped(shared: &Arc<Shared>, job: &SearchJob) -> bool {
    job.cancel.is_cancelled() || shared.stopped.load(Ordering::Acquire)
}

/// The matching hunks of one card, or `None` when neither its path nor a changed line matches.
fn matching_hunks(text: &str, path: &str, card: &Card) -> Option<Vec<u32>> {
    let mut hunks = BTreeSet::new();
    for row in &card.rows {
        let CardRow::Line {
            row,
            hunk,
            left,
            right,
        } = row
        else {
            continue;
        };
        if matches!(row, DiffRow::Equal(..)) {
            continue;
        }
        let hit = [left, right]
            .into_iter()
            .flatten()
            .any(|line| line.to_lowercase().contains(text));
        if hit {
            hunks.insert(*hunk);
        }
    }
    if hunks.is_empty() && !path_matches(text, path) {
        return None;
    }
    Some(hunks.into_iter().collect())
}

/// True when the lower-case `path` contains the lower-case query.
fn path_matches(text: &str, path: &str) -> bool {
    path.to_lowercase().contains(text)
}

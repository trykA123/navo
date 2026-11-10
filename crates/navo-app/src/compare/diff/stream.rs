//! The Stream session: at most four card workers over one settled item list (D26).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use navo_store::CacheError;
use navo_ui::compare_diff::{
    Card, PairRequest, StreamControl, StreamSink, StreamUpdate, ViewedKey,
};

use self::state::{Finish, State, ViewedWrite};
use super::load::LoadInput;
use super::viewed::ViewedMarks;

pub(crate) mod memory;
mod search;
pub(crate) mod state;
mod worker;

/// The four workers D26 allows at once.
const WORKERS: usize = 4;

/// A factory that turns one Stream item into a loadable pair.
pub(crate) type LoadFn = Arc<dyn Fn(&PairRequest) -> Option<LoadInput> + Send + Sync>;

/// A running Stream. Methods only queue work; dropping cancels off the caller.
pub(crate) struct StreamSession {
    shared: Arc<Shared>,
    workers: Option<Vec<JoinHandle<()>>>,
}

/// State and callbacks every worker shares.
pub(super) struct Shared {
    pub state: Mutex<State>,
    /// Taken before the state lock is released and held while sending, so events keep order.
    pub order: Mutex<()>,
    pub ready: Condvar,
    pub sink: StreamSink,
    pub load: LoadFn,
    pub viewed: Arc<dyn ViewedMarks>,
    pub viewed_error: AtomicBool,
    pub stopped: AtomicBool,
}

impl StreamSession {
    /// Start the workers. `budget` is the effective card-body limit, in bytes.
    pub(crate) fn start(
        sink: StreamSink,
        budget: u64,
        load: LoadFn,
        viewed: Arc<dyn ViewedMarks>,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new(budget)),
            order: Mutex::new(()),
            ready: Condvar::new(),
            sink,
            load,
            viewed,
            viewed_error: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        });
        let mut workers = Vec::with_capacity(WORKERS);
        for index in 0..WORKERS {
            let worker = Arc::clone(&shared);
            let spawned = thread::Builder::new()
                .name(format!("compare-stream-{index}"))
                .spawn(move || worker::run(&worker));
            match spawned {
                Ok(handle) => workers.push(handle),
                Err(error) => tracing::warn!(%error, "compare stream worker did not start"),
            }
        }
        Self {
            shared,
            workers: Some(workers),
        }
    }
}

impl StreamControl for StreamSession {
    fn set_items(&self, generation: u64, items: Vec<(PairRequest, bool)>) {
        let mut state = self.lock();
        state.set_items(generation, items);
        let (built, total) = (state.built(), state.total());
        let _order = self.shared.lock_order();
        drop(state);
        (self.shared.sink)(StreamUpdate::Progress {
            generation,
            built,
            total,
        });
        self.shared.ready.notify_all();
    }

    fn focus(&self, ids: Vec<u64>) {
        let mut state = self.lock();
        let held = state.focus(ids);
        let generation = state.generation();
        // A resent card must not overtake an Evicted event a worker sends for it.
        let _order = self.shared.lock_order();
        drop(state);
        for (id, card) in held {
            (self.shared.sink)(StreamUpdate::Card {
                generation,
                id,
                card,
            });
        }
        self.shared.ready.notify_all();
    }

    fn set_viewed(&self, id: u64, key: ViewedKey, viewed: bool) {
        self.lock().set_viewed(ViewedWrite {
            id,
            key: key.0,
            viewed,
        });
        self.shared.ready.notify_all();
    }

    fn search(&self, query: u64, text: String) {
        let mut state = self.lock();
        if text.is_empty() {
            state.search.cancel();
        } else {
            let generation = state.generation();
            state.search.request(query, generation, text);
        }
        drop(state);
        self.shared.ready.notify_all();
    }

    fn reload(&self, id: u64) {
        self.lock().invalidate(id);
        self.shared.ready.notify_all();
    }
}

impl StreamSession {
    /// State bytes held, for the budget test.
    #[cfg(test)]
    pub(crate) fn state_held(&self) -> u64 {
        self.lock().held_bytes()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for StreamSession {
    /// Cancel the workers and join them on a helper thread, never on the caller.
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.lock_state().cancel();
        self.shared.ready.notify_all();
        let Some(workers) = self.workers.take() else {
            return;
        };
        let joined = thread::Builder::new()
            .name("compare-stream-join".to_owned())
            .spawn(move || join_workers(workers));
        if let Err(error) = joined {
            tracing::warn!(%error, "compare stream join thread did not start");
        }
    }
}

/// Join every worker, logging a panic without stopping the rest.
fn join_workers(workers: Vec<JoinHandle<()>>) {
    for worker in workers {
        if worker.join().is_err() {
            tracing::warn!("compare stream worker panicked");
        }
    }
}

impl Shared {
    /// Log the first Viewed store error only, so one failure does not spam the log.
    pub(super) fn warn_viewed(&self, error: &CacheError) {
        if !self.viewed_error.swap(true, Ordering::Relaxed) {
            tracing::warn!(%error, "the compare viewed store failed");
        }
    }

    /// Lock the shared state after a poisoned worker.
    pub(super) fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Take the event-order lock after a poisoned sender.
    pub(super) fn lock_order(&self) -> std::sync::MutexGuard<'_, ()> {
        self.order.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Store a built card in the budget and send it outside the state lock, in event order.
    pub(super) fn store_and_publish(&self, id: u64, generation: u64, card: Arc<Card>) {
        let mut state = self.lock_state();
        let Some(finish) = state.finish(id, generation, card) else {
            return;
        };
        let _order = self.lock_order();
        drop(state);
        self.publish(finish);
    }

    /// Send one built card, its evictions and the progress counters.
    fn publish(&self, finish: Finish) {
        let Finish {
            generation,
            id,
            card,
            evicted,
            built,
            total,
        } = finish;
        let new_evicted = evicted.contains(&id);
        for victim in evicted.into_iter().filter(|victim| *victim != id) {
            (self.sink)(StreamUpdate::Evicted {
                generation,
                id: victim,
            });
        }
        (self.sink)(StreamUpdate::Card {
            generation,
            id,
            card,
        });
        if new_evicted {
            (self.sink)(StreamUpdate::Evicted { generation, id });
        }
        (self.sink)(StreamUpdate::Progress {
            generation,
            built,
            total,
        });
    }
}

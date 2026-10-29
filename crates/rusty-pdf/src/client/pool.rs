use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub(super) struct Pool<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

struct State<T> {
    idle: Option<T>,
    started: bool,
    warming: bool,
    closed: bool,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                idle: None,
                started: false,
                warming: false,
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }
}

impl<T> Pool<T> {
    pub(super) fn warm<E>(&self, load: impl FnOnce() -> Result<T, E>) -> Result<(), E> {
        {
            let mut state = self.lock();
            if state.started || state.closed {
                return Ok(());
            }
            state.started = true;
            state.warming = true;
        }
        let loaded = load();
        let mut state = self.lock();
        state.warming = false;
        let (result, unused) = match loaded {
            Ok(helper) if state.closed || state.idle.is_some() => (Ok(()), Some(helper)),
            Ok(helper) => {
                state.idle = Some(helper);
                (Ok(()), None)
            }
            Err(error) => (Err(error), None),
        };
        drop(state);
        drop(unused);
        self.ready.notify_all();
        result
    }

    pub(super) fn take(&self, wait: Duration) -> Option<T> {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        state.started = true;
        while state.warming {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            state = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        state.idle.take()
    }

    pub(super) fn put(&self, helper: T) {
        let unused = {
            let mut state = self.lock();
            if state.closed || state.idle.is_some() {
                Some(helper)
            } else {
                state.idle = Some(helper);
                None
            }
        };
        drop(unused);
        self.ready.notify_all();
    }

    pub(super) fn wait_for_idle(&self, wait: Duration) -> bool {
        let mut state = self.lock();
        let deadline = Instant::now() + wait;
        while state.idle.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || state.closed {
                return false;
            }
            state = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    pub(super) fn settle(&self, wait: Duration) {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        while state.warming {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            state = self
                .ready
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    pub(super) fn close(&self) {
        let idle = {
            let mut state = self.lock();
            state.closed = true;
            state.idle.take()
        };
        drop(idle);
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;

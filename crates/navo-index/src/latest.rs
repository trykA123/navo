//! A one-slot mailbox: a new value replaces an unread one, so a slow reader
//! never makes the writer queue or block.

use std::sync::{Condvar, Mutex, PoisonError};
#[cfg(test)]
use std::time::{Duration, Instant};

/// Holds at most one value at a time plus a revision counter for waiters.
pub(crate) struct Latest<T> {
    state: Mutex<State<T>>,
    wake: Condvar,
}

struct State<T> {
    value: Option<T>,
    revision: u64,
    open: bool,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                value: None,
                revision: 0,
                open: true,
            }),
            wake: Condvar::new(),
        }
    }
}

impl<T> Latest<T> {
    /// Replaces any unread value with `value`; false when the mailbox is closed.
    pub(crate) fn publish(&self, value: T) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.open {
            return false;
        }
        state.value = Some(value);
        state.revision = state.revision.wrapping_add(1);
        drop(state);
        self.wake.notify_all();
        true
    }

    /// Takes the newest value, or `None` when nothing was published since the
    /// last take.
    pub(crate) fn take(&self) -> Option<T> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .value
            .take()
    }

    /// Takes the newest value, blocking until one is published; `None` once the
    /// mailbox is closed and empty.
    pub(crate) fn next(&self) -> Option<T> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(value) = state.value.take() {
                return Some(value);
            }
            if !state.open {
                return None;
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Refuses new values and wakes blocked readers; a pending value is still
    /// handed out first.
    pub(crate) fn close(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .open = false;
        self.wake.notify_all();
    }

    /// Clones the newest value without consuming it.
    #[cfg(test)]
    pub(crate) fn latest(&self) -> Option<T>
    where
        T: Clone,
    {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .value
            .clone()
    }

    /// The number of publishes so far; with `wait`, lets a reader wait for a
    /// new value instead of spinning on the slot.
    #[cfg(test)]
    pub(crate) fn revision(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revision
    }

    /// Waits up to `timeout` for the value published after `revision`.
    #[cfg(test)]
    pub(crate) fn wait(&self, revision: u64, timeout: Duration) -> Option<T>
    where
        T: Clone,
    {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if state.revision > revision {
                return state.value.clone();
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            state = self
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

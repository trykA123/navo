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
    /// Readers parked in `next`; read by the parked-reader test.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "only the parked-reader test reads it")
    )]
    waiting: usize,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                value: None,
                revision: 0,
                open: true,
                waiting: 0,
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
            state.waiting += 1;
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            state.waiting -= 1;
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

    /// How many readers are parked on the wake condvar.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiting
    }

    /// Whether a value is waiting for a reader.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .value
            .is_some()
    }

    /// Reads the waiting value, if any, without consuming it.
    #[cfg(test)]
    pub(crate) fn peek<R>(&self, read: impl FnOnce(Option<&T>) -> R) -> R {
        read(
            self.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .value
                .as_ref(),
        )
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn a_parked_reader_is_woken_by_a_replacement() {
        let slot = Arc::new(Latest::<u64>::default());
        let reader = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || slot.next())
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while slot.waiting() == 0 {
            assert!(Instant::now() < deadline, "the reader never parked");
            thread::yield_now();
        }
        assert!(slot.publish(7));
        assert_eq!(reader.join().unwrap(), Some(7));
    }
}

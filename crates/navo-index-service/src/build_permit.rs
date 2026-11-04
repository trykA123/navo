use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use navo_index::ntfs::NtfsError;
use navo_index::pipe::CancelToken;

const WAIT: Duration = Duration::from_millis(100);

/// One build at a time per service process; waiting builds do not hold the index lock.
pub(super) struct Permits {
    busy: Mutex<bool>,
    freed: Condvar,
}

impl Permits {
    pub(super) const fn new() -> Self {
        Self {
            busy: Mutex::new(false),
            freed: Condvar::new(),
        }
    }

    /// Waits in 100 ms slices for the current build, returning `Cancelled` once `cancel` fires.
    pub(super) fn acquire(
        &'static self,
        cancel: &CancelToken,
    ) -> Result<BuildPermit<'static>, NtfsError> {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        while *busy {
            if cancel.is_cancelled() {
                return Err(NtfsError::Cancelled);
            }
            busy = self
                .freed
                .wait_timeout(busy, WAIT)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        if cancel.is_cancelled() {
            return Err(NtfsError::Cancelled);
        }
        *busy = true;
        Ok(BuildPermit { permits: self })
    }
}

pub(super) struct BuildPermit<'a> {
    permits: &'a Permits,
}

impl Drop for BuildPermit<'_> {
    fn drop(&mut self) {
        let mut busy = self
            .permits
            .busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *busy = false;
        self.permits.freed.notify_one();
    }
}

#[cfg(windows)]
pub(super) static BUILDS: Permits = Permits::new();

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "permit tests fail immediately when a thread or channel breaks"
    )]

    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use navo_index::ntfs::NtfsError;
    use navo_index::pipe::CancelToken;

    use super::Permits;
    use crate::answer::Shared;

    #[test]
    fn a_second_build_waits_for_the_first_swap() {
        static QUEUE: Permits = Permits::new();
        let (held_tx, held_rx) = mpsc::channel();
        let (swap_tx, swap_rx) = mpsc::channel();
        let (next_tx, next_rx) = mpsc::channel();
        let order = Arc::new(Mutex::new(Vec::new()));
        let first = {
            let order = Arc::clone(&order);
            let cancel = CancelToken::new();
            thread::spawn(move || {
                let permit = QUEUE.acquire(&cancel).unwrap();
                held_tx.send(()).unwrap();
                swap_rx.recv().unwrap();
                order.lock().unwrap().push("swapped");
                drop(permit);
            })
        };
        held_rx.recv().unwrap();
        let second = {
            let order = Arc::clone(&order);
            let cancel = CancelToken::new();
            thread::spawn(move || {
                let _permit = QUEUE.acquire(&cancel).unwrap();
                next_tx.send(()).unwrap();
                order.lock().unwrap().push("next");
            })
        };
        assert!(next_rx.recv_timeout(Duration::from_millis(200)).is_err());
        swap_tx.send(()).unwrap();
        next_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(*order.lock().unwrap(), ["swapped", "next"]);
    }

    #[test]
    fn a_cancelled_waiter_returns_within_a_second() {
        static CANCELLED: Permits = Permits::new();
        let held = CANCELLED.acquire(&CancelToken::new()).unwrap();
        let cancel = CancelToken::new();
        let waiting = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                let started = Instant::now();
                let result = CANCELLED.acquire(&cancel);
                (result.err(), started.elapsed())
            })
        };
        thread::sleep(Duration::from_millis(50));
        cancel.cancel();
        let (error, elapsed) = waiting.join().unwrap();
        assert!(matches!(error, Some(NtfsError::Cancelled)));
        assert!(elapsed < Duration::from_secs(1));
        drop(held);
    }

    #[test]
    fn a_panicking_holder_releases_the_permit() {
        static PANICKED: Permits = Permits::new();
        let result = thread::spawn(|| {
            let _permit = PANICKED.acquire(&CancelToken::new()).unwrap();
            panic!("holder failed");
        })
        .join();
        assert!(result.is_err());
        let permit = PANICKED.acquire(&CancelToken::new()).unwrap();
        drop(permit);
    }

    #[test]
    fn a_waiting_build_does_not_lock_the_index() {
        static BLOCKED: Permits = Permits::new();
        let shared = Shared::default();
        let held = BLOCKED.acquire(&CancelToken::new()).unwrap();
        let waiting = thread::spawn(|| {
            let _permit = BLOCKED.acquire(&CancelToken::new()).unwrap();
        });
        thread::sleep(Duration::from_millis(50));
        assert!(shared.try_write().is_ok());
        drop(held);
        waiting.join().unwrap();
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

#[derive(Debug, Default)]
struct Control {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    wake: Condvar,
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    control: Arc<Control>,
}

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.control.cancelled.store(true, Ordering::Release);
        let _paused = self.paused();
        self.control.wake.notify_all();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.control.cancelled.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn as_atomic_bool(&self) -> &AtomicBool {
        &self.control.cancelled
    }

    pub fn pause(&self) {
        *self.paused() = true;
    }

    pub fn resume(&self) {
        *self.paused() = false;
        self.control.wake.notify_all();
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        *self.paused()
    }

    pub fn wait_while_paused(&self) {
        let mut paused = self.paused();
        while *paused && !self.is_cancelled() {
            paused = self
                .control
                .wake
                .wait(paused)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub fn wait_cancelled(&self, timeout: Duration) -> bool {
        let _wait = self
            .control
            .wake
            .wait_timeout_while(self.paused(), timeout, |_| !self.is_cancelled())
            .unwrap_or_else(PoisonError::into_inner);
        self.is_cancelled()
    }

    fn paused(&self) -> MutexGuard<'_, bool> {
        self.control
            .paused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;

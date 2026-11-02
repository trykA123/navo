use std::sync::{Arc, Mutex};

use super::lock;

type Listener = Arc<dyn Fn() + Send + Sync>;

/// Palette callbacks. They outlive a stop and start of the module and cannot be removed:
/// the palette subscribes once.
#[derive(Default)]
pub(super) struct Listeners(Mutex<Vec<Listener>>);

impl Listeners {
    pub(super) fn add(&self, listener: Listener) {
        lock(&self.0).push(listener);
    }

    /// Calls every listener outside the lock, so one may add another.
    pub(super) fn notify(&self) {
        let all: Vec<Listener> = lock(&self.0).clone();
        for listener in all {
            listener();
        }
    }
}

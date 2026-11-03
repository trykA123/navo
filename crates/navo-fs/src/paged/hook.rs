use std::sync::{Mutex, PoisonError};

pub(super) struct StatHook(Mutex<Option<Box<dyn Fn() + Send>>>);

impl StatHook {
    pub(super) fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub(super) fn set(&self, hook: Box<dyn Fn() + Send>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(hook);
    }

    pub(super) fn run(&self) {
        let hook = self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(hook) = hook {
            hook();
        }
    }
}

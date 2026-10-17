use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::SettingsStore;
use rusty_ui::window_view::SettingsNews;

use super::{describe, report, values_of};

pub struct Reloader {
    store: Arc<Mutex<SettingsStore>>,
    problem: Option<String>,
}

impl Reloader {
    #[must_use]
    pub fn new(store: Arc<Mutex<SettingsStore>>, problem: Option<String>) -> Self {
        Self { store, problem }
    }

    pub fn reload(&mut self) -> Option<SettingsNews> {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let before = store.current().clone();
        if let Some(error) = store.reload().error {
            let message = describe(&error);
            if self.problem.as_ref() == Some(&message) {
                return None;
            }
            report(&message);
            self.problem = Some(message.clone());
            return Some(SettingsNews::Invalid(message));
        }
        let was_broken = self.problem.take().is_some();
        let is_same = store.current() == &before;
        (was_broken || !is_same).then(|| SettingsNews::Applied {
            values: values_of(store.current()),
            generation: store.generation(),
        })
    }
}

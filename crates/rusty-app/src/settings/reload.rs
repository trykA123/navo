use std::sync::{Arc, Mutex};

use rusty_store::SettingsStore;
use rusty_ui::window_view::SettingsNews;

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
        let _ = (&self.store, &self.problem);
        todo!()
    }
}

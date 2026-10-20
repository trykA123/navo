use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_ui::window_view::{Frecent, RecentCommands, SearchHooks};

use crate::files::{SharedDb, lock, now_ms};

const COMMAND_USE_LIMIT: usize = 2_000;

#[derive(Clone)]
pub struct Remember(Arc<AtomicBool>);

impl Default for Remember {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
}

impl Remember {
    pub fn set(&self, on: bool) {
        self.0.store(on, Ordering::Relaxed);
    }

    fn is_on(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

fn report(doing: &str, error: &rusty_store::DbError) {
    eprintln!("ferrite: search memory: {doing}: {error}");
}

fn recent_commands(db: SharedDb, remember: Remember) -> RecentCommands {
    Arc::new(move |limit| {
        if !remember.is_on() {
            return Vec::new();
        }
        lock(&db)
            .recent_commands(limit)
            .inspect_err(|error| report("loading recent commands", error))
            .unwrap_or_default()
    })
}

pub fn hooks(db: Option<SharedDb>, remember: Remember) -> SearchHooks {
    let Some(db) = db else {
        return SearchHooks::none();
    };
    let (weights, record, places, visit, clear) =
        (db.clone(), db.clone(), db.clone(), db.clone(), db.clone());
    let recent_commands = recent_commands(db, remember.clone());
    let (remember_command, remember_visit) = (remember.clone(), remember);
    SearchHooks {
        command_use: Arc::new(move || {
            lock(&weights)
                .command_use(COMMAND_USE_LIMIT, now_ms())
                .inspect_err(|error| report("loading command use", error))
                .unwrap_or_default()
        }),
        recent_commands,
        record_command: Arc::new(move |name| {
            if !remember_command.is_on() {
                return;
            }
            let saved = lock(&record).record_command(name, now_ms());
            let _ = saved.inspect_err(|error| report("saving command use", error));
        }),
        frecent: Arc::new(move |limit| {
            lock(&places)
                .frecent(limit, now_ms())
                .inspect_err(|error| report("loading visits", error))
                .unwrap_or_default()
                .into_iter()
                .map(|found| Frecent {
                    location: found.location,
                    is_folder: found.is_folder,
                    weight: found.weight,
                })
                .collect()
        }),
        record_visit: Arc::new(move |location, is_folder| {
            if !remember_visit.is_on() {
                return;
            }
            let saved = lock(&visit).record_visit(location, is_folder, now_ms());
            let _ = saved.inspect_err(|error| report("saving a visit", error));
        }),
        clear: Arc::new(move || {
            let cleared = lock(&clear).clear_search_history();
            let _ = cleared.inspect_err(|error| report("clearing", error));
        }),
    }
}

#[cfg(test)]
mod tests;

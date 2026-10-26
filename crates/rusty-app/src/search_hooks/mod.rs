mod service;

mod index;

pub use index::hooks as index_hooks;
pub(crate) use service::add_upgrade_notice;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_ui::window_view::{CommandUse, Frecent, Frecents, RecentCommands, SearchHooks};

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
    eprintln!("navo: search memory: {doing}: {error}");
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

fn command_use(db: SharedDb, remember: Remember) -> CommandUse {
    Arc::new(move || {
        if !remember.is_on() {
            return Vec::new();
        }
        lock(&db)
            .command_use(COMMAND_USE_LIMIT, now_ms())
            .inspect_err(|error| report("loading command use", error))
            .unwrap_or_default()
    })
}

fn frecent(db: SharedDb, remember: Remember) -> Frecents {
    Arc::new(move |limit| {
        if !remember.is_on() {
            return Vec::new();
        }
        lock(&db)
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
    })
}

pub fn hooks(db: Option<SharedDb>, remember: Remember) -> SearchHooks {
    let Some(db) = db else {
        return SearchHooks {
            service: service::hooks(),
            ..SearchHooks::none()
        };
    };
    let (record, visit, clear) = (db.clone(), db.clone(), db.clone());
    let command_use = command_use(db.clone(), remember.clone());
    let frecent = frecent(db.clone(), remember.clone());
    let recent_commands = recent_commands(db, remember.clone());
    let (remember_command, remember_visit) = (remember.clone(), remember);
    SearchHooks {
        service: service::hooks(),
        command_use,
        recent_commands,
        record_command: Arc::new(move |name| {
            if !remember_command.is_on() {
                return;
            }
            let saved = lock(&record).record_command(name, now_ms());
            let _ = saved.inspect_err(|error| report("saving command use", error));
        }),
        frecent,
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
        ..SearchHooks::none()
    }
}

#[cfg(test)]
mod tests;

mod folder_totals;
mod service;
mod visits;

pub(crate) use visits::Visits;

mod index;

pub use index::hooks as index_hooks;
#[cfg(feature = "tester")]
pub(crate) use index::search_table;
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

fn frecent(db: SharedDb, visits: Option<Arc<Visits>>, remember: Remember) -> Frecents {
    Arc::new(move |limit| {
        if !remember.is_on() {
            return Vec::new();
        }
        visits
            .as_ref()
            .map_or_else(
                || {
                    lock(&db)
                        .frecent(limit, now_ms())
                        .inspect_err(|error| report("loading visits", error))
                        .unwrap_or_default()
                },
                |visits| visits.rank(limit, now_ms()),
            )
            .into_iter()
            .map(|found| Frecent {
                location: found.location,
                is_folder: found.is_folder,
                weight: found.weight,
            })
            .collect()
    })
}

#[cfg(test)]
pub fn hooks(db: Option<SharedDb>, remember: Remember) -> SearchHooks {
    with_visits(db, remember).0
}

fn start_visits(db: SharedDb) -> Option<Arc<Visits>> {
    Visits::start(db)
        .map(Arc::new)
        .inspect_err(|error| {
            tracing::warn!(%error, "visits worker could not start");
        })
        .ok()
}

fn record_command(db: SharedDb, remember: Remember) -> Arc<dyn Fn(&str) + Send + Sync> {
    Arc::new(move |name| {
        if !remember.is_on() {
            return;
        }
        let saved = lock(&db).record_command(name, now_ms());
        let _ = saved.inspect_err(|error| report("saving command use", error));
    })
}

pub(crate) fn with_visits(
    db: Option<SharedDb>,
    remember: Remember,
) -> (SearchHooks, Option<Arc<Visits>>) {
    let Some(db) = db else {
        return (
            SearchHooks {
                service: service::hooks(),
                folder_totals: folder_totals::query(),
                ..SearchHooks::none()
            },
            None,
        );
    };
    let visits = start_visits(db.clone());
    let (record, visit, clear) = (db.clone(), visits.clone(), visits.clone());
    let command_use = command_use(db.clone(), remember.clone());
    let frecent = frecent(db.clone(), visits.clone(), remember.clone());
    let clear_db = db.clone();
    let recent_commands = recent_commands(db, remember.clone());
    let (remember_command, remember_visit) = (remember.clone(), remember);
    (
        SearchHooks {
            service: service::hooks(),
            folder_totals: folder_totals::query(),
            command_use,
            recent_commands,
            record_command: record_command(record, remember_command),
            frecent,
            record_visit: Arc::new(move |location, is_folder| {
                if !remember_visit.is_on() {
                    return;
                }
                if let Some(visit) = &visit {
                    visit.record(location, is_folder, now_ms());
                }
            }),
            clear: Arc::new(move || {
                if let Some(clear) = &clear {
                    clear.clear();
                } else {
                    let cleared = lock(&clear_db).clear_search_history();
                    let _ = cleared.inspect_err(|error| report("clearing", error));
                }
            }),
            ..SearchHooks::none()
        },
        visits,
    )
}

#[cfg(test)]
mod tests;

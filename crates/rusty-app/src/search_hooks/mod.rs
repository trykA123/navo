use std::sync::Arc;

use rusty_ui::window_view::SearchHooks;

use crate::files::{SharedDb, lock, now_ms};

const COMMAND_USE_LIMIT: usize = 2_000;

fn report(doing: &str, error: &rusty_store::DbError) {
    eprintln!("ferrite: search memory: {doing}: {error}");
}

pub fn hooks(db: Option<SharedDb>) -> SearchHooks {
    let Some(db) = db else {
        return SearchHooks::none();
    };
    let (weights, record, clear) = (db.clone(), db.clone(), db);
    SearchHooks {
        command_use: Arc::new(move || {
            lock(&weights)
                .command_use(COMMAND_USE_LIMIT, now_ms())
                .inspect_err(|error| report("loading command use", error))
                .unwrap_or_default()
        }),
        record_command: Arc::new(move |name| {
            let saved = lock(&record).record_command(name, now_ms());
            let _ = saved.inspect_err(|error| report("saving command use", error));
        }),
        clear: Arc::new(move || {
            let cleared = lock(&clear).clear_search_history();
            let _ = cleared.inspect_err(|error| report("clearing", error));
        }),
    }
}

#[cfg(test)]
mod tests;

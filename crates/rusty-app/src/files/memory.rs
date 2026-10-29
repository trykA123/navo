use std::ops::Deref;
#[cfg(test)]
use std::path::Path;
use std::sync::{Arc, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::marks::SearchHistory;
use rusty_store::{Db, DbError, FileMemory, Stamp};
use rusty_ui::window_view::{FileHooks, FileStamp};

pub type SharedDb = crate::jobs::SharedJournal;

pub(crate) struct MemoryGuard<'a>(MutexGuard<'a, rusty_store::JobJournal>);

impl Deref for MemoryGuard<'_> {
    type Target = Db;

    fn deref(&self) -> &Db {
        self.0.database()
    }
}

fn report(doing: &str, error: &DbError) {
    eprintln!("navo: viewer memory: {doing}: {error}");
}

const KEEP_MEMORIES: usize = 10_000;

#[cfg(test)]
pub fn open(file: &Path) -> Option<SharedDb> {
    open_keeping(file, KEEP_MEMORIES)
}

#[cfg(test)]
fn open_keeping(file: &Path, keep: usize) -> Option<SharedDb> {
    let journal = crate::jobs::open(file)
        .inspect_err(|error| tracing::warn!(%error, "opening viewer memory failed"))
        .ok()?;
    prune_keeping(&journal, keep);
    Some(journal)
}

pub(crate) fn prune(db: &SharedDb) {
    prune_keeping(db, KEEP_MEMORIES);
}

fn prune_keeping(db: &SharedDb, keep: usize) {
    let _ = lock(db)
        .prune_viewer_memory(keep)
        .inspect_err(|error| report("pruning", error));
}

pub(crate) fn lock(db: &SharedDb) -> MemoryGuard<'_> {
    MemoryGuard(crate::jobs::lock(db))
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn stamp(file: FileStamp) -> Stamp {
    Stamp {
        size: file.len,
        modified: file.modified_ms,
    }
}

pub fn attach(hooks: FileHooks, db: SharedDb) -> FileHooks {
    let (load, save, history, remember) = (db.clone(), db.clone(), db.clone(), db);
    FileHooks {
        memory: Arc::new(move |location, file| {
            lock(&load)
                .viewer_memory(location.as_str(), stamp(file))
                .inspect_err(|error| report("loading", error))
                .ok()
                .flatten()
                .map(|memory| (memory.marks, memory.bookmarks))
        }),
        save_memory: Arc::new(move |location, file, marks, bookmarks| {
            let memory = FileMemory {
                marks: marks.clone(),
                bookmarks: bookmarks.clone(),
            };
            let saved =
                lock(&save).save_viewer_memory(location.as_str(), stamp(file), &memory, now_ms());
            let _ = saved.inspect_err(|error| report("saving", error));
        }),
        history: Arc::new(move || {
            lock(&history)
                .search_history()
                .inspect_err(|error| report("loading search history", error))
                .unwrap_or_else(|_| SearchHistory::default())
        }),
        remember: Arc::new(move |needle| {
            let saved = lock(&remember).remember_search(needle, now_ms());
            let _ = saved.inspect_err(|error| report("saving a search", error));
        }),
        ..hooks
    }
}

#[cfg(test)]
mod tests;

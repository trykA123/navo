use std::cell::Cell;
use std::path::PathBuf;

use tempfile::TempDir;

use super::{ClipError, ClipStore, Limits};
use crate::model::{Content, Entry, Saved, SavedKind, Scope, Sealed, Transfer};

mod aside;
mod erase;
mod history;
mod ids;
mod listing;
mod migrate;
mod notes;
mod safety;
mod saved;
mod wipe;

thread_local! {
    static FAIL_CLEANUP: Cell<bool> = const { Cell::new(false) };
    static FAIL_SEED: Cell<bool> = const { Cell::new(false) };
}

/// Fails seeding the fresh database of a Delete all while `set_fail_seed(true)` holds.
pub(super) fn fail_seed() -> Result<(), ClipError> {
    if FAIL_SEED.get() {
        return Err(ClipError::Io(std::io::Error::other(
            "simulated seed failure",
        )));
    }
    Ok(())
}

fn set_fail_seed(on: bool) {
    FAIL_SEED.set(on);
}

/// Fails an owed upgrade cleanup while `set_fail_cleanup(true)` holds on this thread.
pub(super) fn fail_cleanup() -> Result<(), ClipError> {
    if FAIL_CLEANUP.get() {
        return Err(ClipError::Io(std::io::Error::other(
            "simulated cleanup failure",
        )));
    }
    Ok(())
}

fn set_fail_cleanup(on: bool) {
    FAIL_CLEANUP.set(on);
}

fn temp_store(limits: Limits) -> (TempDir, ClipStore) {
    let dir = TempDir::new().unwrap();
    let store = ClipStore::open(&dir.path().join("clipboard.db"), limits).unwrap();
    (dir, store)
}

fn entry(content: Content, at_ms: i64) -> Entry {
    Entry {
        content,
        source_app: Some("app.exe".into()),
        first_ms: at_ms,
        last_ms: at_ms,
        uses: 1,
    }
}

fn text(body: &str, at_ms: i64) -> Entry {
    entry(Content::Text(body.to_owned()), at_ms)
}

fn path_entry(paths: &[&str], transfer: Transfer, at_ms: i64) -> Entry {
    entry(
        Content::Paths {
            paths: paths.iter().map(PathBuf::from).collect(),
            transfer,
        },
        at_ms,
    )
}

fn item(kind: SavedKind, body: &str) -> Saved {
    Saved {
        id: 0,
        kind,
        title: format!("{kind:?} title"),
        body: Some(body.to_owned()),
        user: Some("admin".into()),
        sealed: None,
        scope: Scope::Global,
        created_ms: 100,
        updated_ms: 200,
    }
}

fn secret(sealed: &[u8]) -> Saved {
    Saved {
        body: None,
        sealed: Some(Sealed(sealed.to_vec())),
        ..item(SavedKind::Secret, "")
    }
}

fn folder(path: &str) -> Scope {
    Scope::Folder(PathBuf::from(path))
}

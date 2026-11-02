use std::path::PathBuf;

use tempfile::TempDir;

use super::{ClipStore, Limits};
use crate::model::{Content, Entry, Saved, SavedKind, Scope, Sealed, Transfer};

mod aside;
mod erase;
mod history;
mod listing;
mod migrate;
mod notes;
mod safety;
mod saved;
mod wipe;

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

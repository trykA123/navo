//! Saved items.

use std::path::PathBuf;

use rusqlite::{Row, params};

use crate::limits::MAX_TITLE_BYTES;
use crate::model::{Saved, SavedKind, Scope, Sealed};

use super::{ClipError, ClipStore};

/// Saved items that read back, and the ids of rows that did not, so they can be deleted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Listed {
    pub items: Vec<Saved>,
    pub skipped: Vec<i64>,
}

impl ClipStore {
    /// Lists saved items in the order they were first saved. One unreadable row lands in
    /// `skipped` and never hides the rest.
    pub fn saved(&self) -> Result<Listed, ClipError> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, title, body, user, sealed, scope, created_ms, updated_ms
             FROM saved
             ORDER BY id",
        )?;
        let mut rows = statement.query([])?;
        let mut listed = Listed::default();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            match decode(row) {
                Ok(item) => listed.items.push(item),
                Err(_) => listed.skipped.push(id),
            }
        }
        Ok(listed)
    }

    /// Stores a new item and returns its id; `item.id` is ignored.
    /// Fails with [`ClipError::TooManySaved`] when the list is full.
    pub fn save(&self, item: &Saved) -> Result<i64, ClipError> {
        self.check(item)?;
        let cap = self.limits.max_saved_rows;
        let added = self.conn.execute(
            "INSERT INTO saved (kind, title, body, user, sealed, scope, created_ms, updated_ms)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
             WHERE (SELECT count(*) FROM saved) < ?9",
            params![
                kind_code(item.kind),
                item.title,
                item.body,
                item.user,
                item.sealed.as_ref().map(|sealed| sealed.0.as_slice()),
                scope_column(&item.scope),
                item.created_ms,
                item.updated_ms,
                i64::try_from(cap).unwrap_or(i64::MAX)
            ],
        )?;
        if added == 0 {
            return Err(ClipError::TooManySaved { cap });
        }
        Ok(self.conn.last_insert_rowid())
    }

    /// Replaces the item with `item.id`, keeping its stored `created_ms`.
    /// Fails with [`ClipError::Missing`] when the item is gone. The old text leaves the log at
    /// once; on [`ClipError::CheckpointBusy`] the item is already replaced.
    pub fn update(&self, item: &Saved) -> Result<(), ClipError> {
        self.check(item)?;
        let changed = self.conn.execute(
            "UPDATE saved
             SET kind = ?2, title = ?3, body = ?4, user = ?5, sealed = ?6, scope = ?7,
                 updated_ms = ?8
             WHERE id = ?1",
            params![
                item.id,
                kind_code(item.kind),
                item.title,
                item.body,
                item.user,
                item.sealed.as_ref().map(|sealed| sealed.0.as_slice()),
                scope_column(&item.scope),
                item.updated_ms
            ],
        )?;
        if changed == 0 {
            return Err(ClipError::Missing);
        }
        self.checkpoint()
    }

    /// Deletes the item with `id`, also one that `saved` skipped as unreadable, and empties
    /// the log. On [`ClipError::CheckpointBusy`] the item is already deleted.
    pub fn delete_saved(&self, id: i64) -> Result<(), ClipError> {
        self.conn.execute("DELETE FROM saved WHERE id = ?1", [id])?;
        self.checkpoint()
    }

    fn check(&self, item: &Saved) -> Result<(), ClipError> {
        let sealed = item.sealed.as_ref().map(|sealed| sealed.0.as_slice());
        shape(item.kind, item.body.is_some(), sealed)?;
        let scope = scope_column(&item.scope);
        if scope.as_deref() == Some("") {
            return Err(ClipError::Malformed("folder scope without a path"));
        }
        let cap = self.limits.max_saved_bytes;
        within_cap(item.title.len(), MAX_TITLE_BYTES)?;
        within_cap(item.user.as_ref().map_or(0, String::len), MAX_TITLE_BYTES)?;
        within_cap(item.body.as_ref().map_or(0, String::len), cap)?;
        within_cap(sealed.map_or(0, <[u8]>::len), cap)?;
        within_cap(scope.as_ref().map_or(0, String::len), cap)
    }
}

pub(super) fn within_cap(bytes: usize, cap: usize) -> Result<(), ClipError> {
    if bytes > cap {
        return Err(ClipError::TooLarge { bytes, cap });
    }
    Ok(())
}

fn decode(row: &Row<'_>) -> Result<Saved, ClipError> {
    let kind = decode_kind(row.get(1)?)?;
    let body: Option<String> = row.get(3)?;
    let sealed: Option<Vec<u8>> = row.get(5)?;
    shape(kind, body.is_some(), sealed.as_deref())?;
    let scope: Option<String> = row.get(6)?;
    if scope.as_deref() == Some("") {
        return Err(ClipError::Malformed("folder scope without a path"));
    }
    Ok(Saved {
        id: row.get(0)?,
        kind,
        title: row.get(2)?,
        body,
        user: row.get(4)?,
        sealed: sealed.map(Sealed),
        scope: scope.map_or(Scope::Global, |path| Scope::Folder(PathBuf::from(path))),
        created_ms: row.get(7)?,
        updated_ms: row.get(8)?,
    })
}

/// A secret has non-empty sealed bytes and no body; any other item has no sealed bytes.
fn shape(kind: SavedKind, has_body: bool, sealed: Option<&[u8]>) -> Result<(), ClipError> {
    let has_sealed = sealed.is_some_and(|bytes| !bytes.is_empty());
    match (kind == SavedKind::Secret, has_body, has_sealed) {
        (true, true, _) => Err(ClipError::PlainSecret),
        (true, false, false) => Err(ClipError::SecretNotSealed),
        (false, _, _) if sealed.is_some() => {
            Err(ClipError::Malformed("sealed bytes on a plain item"))
        }
        _ => Ok(()),
    }
}

fn kind_code(kind: SavedKind) -> i64 {
    match kind {
        SavedKind::Snippet => 0,
        SavedKind::Command => 1,
        SavedKind::Path => 2,
        SavedKind::Secret => 3,
        SavedKind::Note => 4,
    }
}

fn decode_kind(code: i64) -> Result<SavedKind, ClipError> {
    match code {
        0 => Ok(SavedKind::Snippet),
        1 => Ok(SavedKind::Command),
        2 => Ok(SavedKind::Path),
        3 => Ok(SavedKind::Secret),
        4 => Ok(SavedKind::Note),
        _ => Err(ClipError::Malformed("unknown saved kind")),
    }
}

fn scope_column(scope: &Scope) -> Option<String> {
    match scope {
        Scope::Global => None,
        Scope::Folder(path) => Some(path.to_string_lossy().into_owned()),
    }
}

//! Folder notes and the global note, keyed by [`folder_key`].

use std::path::PathBuf;

use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};

use crate::model::{NoteRow, Scope};
use crate::scope::folder_key;

use super::saved::within_cap;
use super::{ClipError, ClipStore};

const GLOBAL_KEY: &str = "";

/// Notes that read back, and how many rows did not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NoteListed {
    pub notes: Vec<NoteRow>,
    pub skipped: usize,
}

struct NoteKey {
    key: String,
    display: String,
}

impl ClipStore {
    /// The note for `scope`; folders compare as [`folder_key`] says.
    pub fn note(&self, scope: &Scope) -> Result<Option<String>, ClipError> {
        let note = self.note_key(scope)?;
        Ok(self
            .conn
            .query_row("SELECT body FROM notes WHERE key = ?1", [note.key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Every readable note, the global one first, then by folder key. Folders need not exist.
    /// One unreadable row is counted in `skipped` and never hides the rest; remove such rows
    /// with [`ClipStore::delete_unreadable_notes`].
    pub fn notes(&self) -> Result<NoteListed, ClipError> {
        let (notes, bad) = read_notes(&self.conn)?;
        Ok(NoteListed {
            notes,
            skipped: bad.len(),
        })
    }

    /// Deletes every note row that `notes` skips, empties the log and returns how many it
    /// removed. On [`ClipError::CheckpointBusy`] the rows are already deleted.
    pub fn delete_unreadable_notes(&self) -> Result<usize, ClipError> {
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let (_, bad) = read_notes(&transaction)?;
        let mut deleted = 0;
        for rowid in bad {
            deleted += transaction.execute("DELETE FROM notes WHERE rowid = ?1", [rowid])?;
        }
        transaction.commit()?;
        self.checkpoint()?;
        Ok(deleted)
    }

    /// Sets the note for `scope` at `at_ms`, keeping the path as written; an empty `body`
    /// deletes it. The old text leaves the log at once; on [`ClipError::CheckpointBusy`] the
    /// note is already changed.
    pub fn set_note(&self, scope: &Scope, body: &str, at_ms: i64) -> Result<(), ClipError> {
        let note = self.note_key(scope)?;
        if body.is_empty() {
            self.conn
                .execute("DELETE FROM notes WHERE key = ?1", [note.key])?;
            return self.checkpoint();
        }
        within_cap(body.len(), self.limits.max_saved_bytes)?;
        self.conn.execute(
            "INSERT INTO notes (key, display, body, updated_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET
                 display = excluded.display,
                 body = excluded.body,
                 updated_ms = excluded.updated_ms",
            params![note.key, note.display, body, at_ms],
        )?;
        self.checkpoint()
    }

    /// Moves the note of folder `from` to folder `to` at `at_ms`. Refuses with
    /// [`ClipError::Malformed`] unless both scopes are folders, with [`ClipError::NoteExists`]
    /// when `to` is another folder that already has a note, and with
    /// [`ClipError::NoteMissing`] when `from` has none.
    pub fn move_note(&self, from: &Scope, to: &Scope, at_ms: i64) -> Result<(), ClipError> {
        if !matches!((from, to), (Scope::Folder(_), Scope::Folder(_))) {
            return Err(ClipError::Malformed("only folder notes move"));
        }
        let source = self.note_key(from)?;
        let target = self.note_key(to)?;
        let moved = self.conn.execute(
            "UPDATE notes SET key = ?2, display = ?3, updated_ms = ?4 WHERE key = ?1",
            params![source.key, target.key, target.display, at_ms],
        );
        match moved {
            Ok(0) => Err(ClipError::NoteMissing),
            Ok(_) => Ok(()),
            Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => {
                Err(ClipError::NoteExists)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn note_key(&self, scope: &Scope) -> Result<NoteKey, ClipError> {
        let Scope::Folder(path) = scope else {
            return Ok(NoteKey {
                key: GLOBAL_KEY.to_owned(),
                display: String::new(),
            });
        };
        let display = path.to_string_lossy().into_owned();
        if display.is_empty() {
            return Err(ClipError::Malformed("folder note without a path"));
        }
        within_cap(display.len(), self.limits.max_saved_bytes)?;
        Ok(NoteKey {
            key: folder_key(path),
            display,
        })
    }
}

fn read_notes(conn: &Connection) -> Result<(Vec<NoteRow>, Vec<i64>), ClipError> {
    let mut statement = conn
        .prepare("SELECT rowid, key, display, body, updated_ms FROM notes ORDER BY key, rowid")?;
    let mut rows = statement.query([])?;
    let (mut notes, mut bad) = (Vec::new(), Vec::new());
    while let Some(row) = rows.next()? {
        let rowid: i64 = row.get(0)?;
        match decode_note(row) {
            Ok(note) => notes.push(note),
            Err(_) => bad.push(rowid),
        }
    }
    Ok((notes, bad))
}

/// The display path is read only for folder notes; the global note ignores it.
fn decode_note(row: &Row<'_>) -> rusqlite::Result<NoteRow> {
    let key: String = row.get(1)?;
    let scope = if key == GLOBAL_KEY {
        Scope::Global
    } else {
        Scope::Folder(PathBuf::from(row.get::<_, String>(2)?))
    };
    Ok(NoteRow {
        scope,
        body: row.get(3)?,
        updated_ms: row.get(4)?,
    })
}

use std::fmt;

use crate::model::{Saved, Scope};
use crate::search::Rows;
use crate::store::Stored;

/// One note in the list, without its body; ask the service for the body.
#[derive(Clone, PartialEq, Eq)]
pub struct NoteHeader {
    pub scope: Scope,
    pub updated_ms: i64,
}

impl fmt::Debug for NoteHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteHeader")
            .field("updated_ms", &self.updated_ms)
            .finish_non_exhaustive()
    }
}

/// The rows at one revision. Shared, never changed.
#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: u64,
    /// Newest first, up to `Limits::max_entries`.
    pub history: Vec<Stored>,
    /// In the order first saved. A secret carries its title and user only: no body, no
    /// sealed bytes.
    pub saved: Vec<Saved>,
    /// Saved rows that did not read back.
    pub saved_skipped: usize,
    /// The global note first, then by folder.
    pub notes: Vec<NoteHeader>,
    /// Note rows that did not read back.
    pub notes_skipped: usize,
}

impl Snapshot {
    pub(super) fn empty() -> Self {
        Self {
            revision: 0,
            history: Vec::new(),
            saved: Vec::new(),
            saved_skipped: 0,
            notes: Vec::new(),
            notes_skipped: 0,
        }
    }

    pub fn rows(&self) -> Rows<'_> {
        Rows {
            history: &self.history,
            saved: &self.saved,
        }
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("revision", &self.revision)
            .field("history", &self.history.len())
            .field("saved", &self.saved.len())
            .field("saved_skipped", &self.saved_skipped)
            .field("notes", &self.notes.len())
            .field("notes_skipped", &self.notes_skipped)
            .finish()
    }
}

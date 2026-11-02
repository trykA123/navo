use std::fmt;
use std::path::PathBuf;

/// Whether a file list was copied or cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Copy,
    Cut,
}

/// What one clipboard change carried.
#[derive(Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Paths {
        paths: Vec<PathBuf>,
        transfer: Transfer,
    },
}

impl Content {
    /// Text bytes, or the path count.
    pub fn len(&self) -> usize {
        match self {
            Self::Text(text) => text.len(),
            Self::Paths { paths, .. } => paths.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.trim().is_empty(),
            Self::Paths { paths, .. } => paths.is_empty(),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "Text",
            Self::Paths { .. } => "Paths",
        }
    }
}

impl fmt::Debug for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(len {})", self.kind_name(), self.len())
    }
}

/// A clipboard change as the watcher read it.
#[derive(Clone, PartialEq, Eq)]
pub struct Captured {
    pub content: Content,
    pub source_app: Option<String>,
    pub at_ms: i64,
}

impl fmt::Debug for Captured {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Captured")
            .field("content", &self.content)
            .field("at_ms", &self.at_ms)
            .finish_non_exhaustive()
    }
}

/// One history row.
#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub content: Content,
    pub source_app: Option<String>,
    pub first_ms: i64,
    pub last_ms: i64,
    pub uses: u32,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("content", &self.content)
            .field("first_ms", &self.first_ms)
            .field("last_ms", &self.last_ms)
            .field("uses", &self.uses)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedKind {
    Snippet,
    Command,
    Path,
    Secret,
    Note,
}

/// Where a saved item applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Global,
    Folder(PathBuf),
}

/// Secret bytes the OS already sealed; the store never sees the plain secret.
/// Sealed bytes are larger than the plain text, so the caller must leave headroom below
/// `max_saved_bytes`.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed(pub Vec<u8>);

impl fmt::Debug for Sealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sealed(len {})", self.0.len())
    }
}

/// A saved item. A secret has no `body`, only `sealed` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Saved {
    pub id: i64,
    pub kind: SavedKind,
    pub title: String,
    pub body: Option<String>,
    pub user: Option<String>,
    pub sealed: Option<Sealed>,
    pub scope: Scope,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl fmt::Debug for Saved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Saved")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("body_len", &self.body.as_ref().map(String::len))
            .field("sealed", &self.sealed)
            .finish_non_exhaustive()
    }
}

/// One folder note, or the global note. `scope` is the folder path as last written.
#[derive(Clone, PartialEq, Eq)]
pub struct NoteRow {
    pub scope: Scope,
    pub body: String,
    pub updated_ms: i64,
}

impl fmt::Debug for NoteRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteRow")
            .field("body_len", &self.body.len())
            .field("updated_ms", &self.updated_ms)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_kind_and_length_only() {
        let text = "canary-body".to_owned();
        let captured = Captured {
            content: Content::Text(text.clone()),
            source_app: Some("canary-app".into()),
            at_ms: 1,
        };
        let entry = Entry {
            content: captured.content.clone(),
            source_app: captured.source_app.clone(),
            first_ms: 1,
            last_ms: 1,
            uses: 1,
        };
        let saved = Saved {
            id: 1,
            kind: SavedKind::Secret,
            title: "canary-title".into(),
            body: Some(text),
            user: Some("canary-user".into()),
            sealed: Some(Sealed(b"canary-sealed".to_vec())),
            scope: Scope::Folder("canary-folder".into()),
            created_ms: 1,
            updated_ms: 1,
        };
        let note = NoteRow {
            scope: saved.scope.clone(),
            body: "canary-note".into(),
            updated_ms: 1,
        };
        let shown = format!("{captured:?} {entry:?} {saved:?} {note:?}");
        assert!(!shown.contains("canary"), "{shown}");
        assert!(shown.contains("Text(len 11)"));
    }
}

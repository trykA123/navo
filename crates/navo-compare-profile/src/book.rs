//! Recent sessions for the app: opens the profile's sessions store, records each compare as a
//! session, re-reads the start rows and reads a stored compare back. No GUI.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths::{self, ProfileError};
use crate::recent::{self, StartRows};
use crate::session::{Rules, SESSION_VERSION, Session, SessionKind, SideRef};
use crate::session_store::{SessionStore, StoreError};

/// Two sides (paths or Git revisions) and their `.gitignore` rule: one compare to record, or one to open again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub left: SideRef,
    pub right: SideRef,
    pub respect_gitignore: bool,
}
impl Visit {
    /// Makes a visit of two path sides with the `.gitignore` rule.
    pub fn paths(left: PathBuf, right: PathBuf, respect_gitignore: bool) -> Visit {
        Visit {
            left: SideRef::path(left),
            right: SideRef::path(right),
            respect_gitignore,
        }
    }
}
/// The sessions store of an opened profile and the start rows last read from it.
pub struct SessionBook {
    store: SessionStore,
    rows: StartRows,
}
impl SessionBook {
    /// Opens the profile and store, then refreshes. Fails with `"Recent sessions are off: …"`.
    pub fn open(explicit: Option<&Path>, now_s: i64) -> Result<SessionBook, String> {
        let profile = paths::open_profile(explicit)
            .map_err(|error| format!("Recent sessions are off: {}.", profile_text(&error)))?;
        let mut book = SessionBook {
            store: SessionStore::open(profile.sessions_dir()),
            rows: StartRows::default(),
        };
        book.refresh(now_s)?;
        Ok(book)
    }
    /// Returns the start rows from the last successful read.
    pub fn rows(&self) -> &StartRows {
        &self.rows
    }
    /// Records `visit`, bumping a match or adding one, then refreshes. Fails with `"Could not save this compare to Recent: …"`.
    pub fn record(&mut self, visit: &Visit, now_ms: u64) -> Result<(), String> {
        let now_s = i64::try_from(now_ms / 1000).unwrap_or(i64::MAX);
        let sessions = self.store.list().map_err(|error| save_text(&error))?;
        let mut session = match sessions
            .into_iter()
            .find(|session| same_pair(session, visit))
        {
            Some(session) => session,
            None => new_session(self.store.new_id(now_ms), visit, now_s),
        };
        session.opened = now_s;
        let saved = self
            .store
            .save(&session)
            .and_then(|()| self.store.prune_recent().map(|_| ()));
        let refreshed = self.refresh(now_s);
        match saved {
            Err(error) => Err(save_text(&error)),
            Ok(()) => refreshed,
        }
    }
    /// Loads session `id` as a visit of its two sides (paths or Git revisions). Fails with `"Could not open this session: …"` for a bad read or a non-pair.
    pub fn load(&self, id: &str) -> Result<Visit, String> {
        let session = self
            .store
            .load(id)
            .map_err(|error| format!("Could not open this session: {}.", store_text(&error)))?;
        if session.kind != SessionKind::Pair {
            return Err("Could not open this session: it does not hold two paths.".to_string());
        }
        Ok(Visit {
            left: session.left,
            right: session.right,
            respect_gitignore: session.rules.respect_gitignore,
        })
    }
    /// Deletes unnamed session `id` from the profile, refreshes and returns it. Fails with `"Named sessions are not removed from Recent."` or `"Could not remove this session from Recent: …"`.
    pub fn forget(&mut self, id: &str, now_s: i64) -> Result<Session, String> {
        let remove_text = |error: &StoreError| {
            format!(
                "Could not remove this session from Recent: {}.",
                store_text(error)
            )
        };
        let session = self.store.load(id).map_err(|error| remove_text(&error))?;
        if session.name.is_some() {
            return Err("Named sessions are not removed from Recent.".to_string());
        }
        self.store.delete(id).map_err(|error| remove_text(&error))?;
        self.refresh(now_s)?;
        Ok(session)
    }
    /// Saves `session` back under its own id (the undo of `forget`) and refreshes, without pruning. Fails with `"Could not restore this session: …"`.
    pub fn restore(&mut self, session: &Session, now_s: i64) -> Result<(), String> {
        self.store
            .save(session)
            .map_err(|error| format!("Could not restore this session: {}.", store_text(&error)))?;
        self.refresh(now_s)
    }
    /// Reads the stored sessions at `now_s` and rebuilds the start rows, keeping them on error.
    pub fn refresh(&mut self, now_s: i64) -> Result<(), String> {
        let sessions = self
            .store
            .list()
            .map_err(|error| format!("Could not read recent sessions: {}.", store_text(&error)))?;
        self.rows = recent::start_rows(&sessions, now_s);
        Ok(())
    }
}
/// Milliseconds since 1970 from the system clock; 0 before it and `u64::MAX` past its range.
pub fn now_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}
/// Describes a store error for the user, without a trailing full stop.
pub fn store_text(error: &StoreError) -> String {
    match error {
        StoreError::Io { path, error } => format!("{}: {error}", path.display()),
        StoreError::BadId(id) => format!("the id {id:?} is not valid"),
        StoreError::BadSide => "a side or rule cannot be stored".to_string(),
        StoreError::NotStorable => "pasted texts are never stored".to_string(),
        StoreError::Newer { id, found } => {
            format!("session {id} has version {found}, newer than this app reads")
        }
        StoreError::Broken { id } => format!("session {id} cannot be read"),
    }
}
/// Describes a profile error for the user, without a trailing full stop.
pub fn profile_text(error: &ProfileError) -> String {
    match error {
        ProfileError::NoHome => "no home folder is set".to_string(),
        ProfileError::Io { path, error } => format!("{}: {error}", path.display()),
        ProfileError::Newer { found, supported } => {
            format!("the profile has version {found}; this app reads version {supported}")
        }
        ProfileError::Broken { path } => format!("{} cannot be read", path.display()),
    }
}
/// Wraps a store error in the text shown when a compare cannot be saved.
fn save_text(error: &StoreError) -> String {
    format!(
        "Could not save this compare to Recent: {}.",
        store_text(error)
    )
}
/// Returns true when `session` is the two sides (paths or Git revisions) of `visit` with the same `.gitignore` rule.
fn same_pair(session: &Session, visit: &Visit) -> bool {
    session.kind == SessionKind::Pair
        && session.left == visit.left
        && session.right == visit.right
        && session.rules.respect_gitignore == visit.respect_gitignore
}
/// Builds a new unnamed pair session for `visit`'s two sides (paths or Git revisions), opened at `now_s`.
fn new_session(id: String, visit: &Visit, now_s: i64) -> Session {
    Session {
        version: SESSION_VERSION,
        id,
        name: None,
        kind: SessionKind::Pair,
        left: visit.left.clone(),
        right: visit.right.clone(),
        rules: Rules {
            whitespace: "keep".to_string(),
            ignore_bom: false,
            ignore_line_endings: false,
            ignore_case: false,
            respect_gitignore: visit.respect_gitignore,
        },
        filter: "all".to_string(),
        opened: now_s,
        view: Vec::new(),
    }
}

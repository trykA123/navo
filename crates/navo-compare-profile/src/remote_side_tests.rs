//! Tests for remote https sides in stored sessions: TOML round trip, validation
//! and the Recent text built from them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::recent::side_text;
use crate::session::{Rules, SESSION_VERSION, Session, SessionKind, SideKind, SideRef};
use crate::session_store::{SessionStore, StoreError};

const URL: &str = "https://github.com/owner/repo";

/// A unique temporary sessions folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty folder named after `name` and a counter.
    fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let folder = format!("{}-remote-{name}-{count}", std::process::id());
        let path = std::env::temp_dir().join(folder);
        std::fs::create_dir_all(&path).expect("temp folder");
        TempDir { path }
    }
}

impl Drop for TempDir {
    /// Removes the folder and everything in it, ignoring errors.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Returns a session store whose sessions folder is under `temp`.
fn store(temp: &TempDir) -> SessionStore {
    SessionStore::open(temp.path.join("sessions"))
}

/// Builds a valid pair session with the two sides and no name.
fn session(left: SideRef, right: SideRef) -> Session {
    Session {
        version: SESSION_VERSION,
        id: "aaaa".to_string(),
        name: None,
        kind: SessionKind::Pair,
        left,
        right,
        rules: Rules {
            whitespace: "keep".to_string(),
            ignore_bom: true,
            ignore_line_endings: false,
            ignore_case: true,
            respect_gitignore: false,
        },
        filter: "all".to_string(),
        opened: 1_700_000_000,
        view: Vec::new(),
    }
}

/// Returns true when the store refuses the session as a bad side.
fn refused(left: SideRef, right: SideRef) -> bool {
    let temp = TempDir::new("refused");
    matches!(
        store(&temp).save(&session(left, right)),
        Err(StoreError::BadSide)
    )
}

#[test]
fn a_remote_side_round_trips_through_toml() {
    let left = SideRef::remote(URL.to_string(), "main".to_string());
    let right = SideRef::path(std::env::temp_dir().join("right"));
    let original = session(left, right);
    let text = toml::to_string(&original).expect("serialise session");
    assert!(text.contains("kind = \"remote\""));
    assert!(text.contains(&format!("url = \"{URL}\"")));
    let parsed: Session = toml::from_str(&text).expect("parse session");
    assert_eq!(parsed, original);
    assert_eq!(parsed.left.kind, SideKind::Remote);
    let temp = TempDir::new("round-trip");
    assert!(store(&temp).save(&parsed).is_ok());
}

#[test]
fn a_non_canonical_url_is_refused() {
    let left = SideRef::remote(
        "HTTPS://GitHub.com:443/owner/repo/".to_string(),
        "main".to_string(),
    );
    let right = SideRef::path(std::env::temp_dir().join("right"));
    assert!(refused(left, right));
}

#[test]
fn a_bad_rev_is_refused() {
    let right = || SideRef::path(std::env::temp_dir().join("right"));
    assert!(refused(
        SideRef::remote(URL.to_string(), "a..b".to_string()),
        right()
    ));
    assert!(!refused(
        SideRef::remote(URL.to_string(), "HEAD".to_string()),
        right()
    ));
}

#[test]
fn a_remote_side_needs_an_empty_path_and_a_url() {
    let right = || SideRef::path(std::env::temp_dir().join("right"));
    let mut absolute = SideRef::remote(URL.to_string(), "main".to_string());
    absolute.path = PathBuf::from("/abs");
    assert!(refused(absolute, right()));
    let mut missing_url = SideRef::remote(URL.to_string(), "main".to_string());
    missing_url.url = None;
    assert!(refused(missing_url, right()));
    let mut path_with_url = SideRef::path(std::env::temp_dir().join("left"));
    path_with_url.url = Some(URL.to_string());
    assert!(refused(path_with_url, right()));
}

#[test]
fn recent_shows_the_remote_address() {
    let remote = SideRef::remote(URL.to_string(), "main".to_string());
    assert_eq!(side_text(&remote), format!("{URL}@main"));
}

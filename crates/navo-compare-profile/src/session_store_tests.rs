//! Store tests for fixed version writes, entry filtering and refusal order.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::brand;
use crate::session::{Rules, SESSION_VERSION, Session, SessionKind, SideKind, SideRef};
use crate::session_store::{SessionStore, StoreError};

/// A unique temporary folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty folder named after the test and a counter.
    fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let folder = format!(
            "{}-store-{}-{name}-{count}",
            brand::SLUG,
            std::process::id()
        );
        let path = std::env::temp_dir().join(folder);
        std::fs::create_dir_all(&path).expect("temp folder");
        TempDir { path }
    }

    /// Returns the path of `name` inside the folder.
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    /// Removes the folder and everything in it, ignoring errors.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Returns a store whose sessions folder is under `temp`.
fn store(temp: &TempDir) -> SessionStore {
    SessionStore::open(temp.join("sessions"))
}

/// Builds a valid pair session with id `id` and absolute sides.
fn sample(id: &str) -> Session {
    let near = std::env::temp_dir();
    Session {
        version: SESSION_VERSION,
        id: id.to_string(),
        name: None,
        kind: SessionKind::Pair,
        left: SideRef {
            kind: SideKind::Path,
            path: near.join("left"),
            rev: None,
            url: None,
        },
        right: SideRef {
            kind: SideKind::Git,
            path: near.join("repo"),
            rev: Some("main".to_string()),
            url: None,
        },
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

/// Serialises `session` as the store would write it (used by the Unix-only link test).
#[cfg(unix)]
fn session_text(session: &Session) -> String {
    toml::to_string(session).expect("serialise session")
}

#[test]
fn save_writes_the_current_version_whatever_the_struct_holds() {
    let temp = TempDir::new("version");
    let store = store(&temp);
    for version in [7, 0] {
        let mut session = sample("aaaa");
        session.version = version;
        store.save(&session).expect("save");
        let loaded = store.load("aaaa").expect("load");
        assert_eq!(loaded.version, SESSION_VERSION);
    }
}

#[test]
fn texts_is_refused_before_the_side_check() {
    let temp = TempDir::new("texts-side");
    let store = store(&temp);
    let mut session = sample("aaaa");
    session.kind = SessionKind::Texts;
    session.left.path = PathBuf::from("relative");
    assert!(matches!(store.save(&session), Err(StoreError::NotStorable)));
}

#[test]
fn list_skips_directories_named_like_sessions() {
    let temp = TempDir::new("dir-entry");
    let store = store(&temp);
    store.save(&sample("abc1")).expect("first save");
    store.save(&sample("abc2")).expect("second save");
    std::fs::create_dir(temp.join("sessions").join("abc-1.toml")).expect("directory");
    assert_eq!(store.list().expect("list").len(), 2);
    assert!(store.prune_recent().is_ok());
}

#[cfg(unix)]
#[test]
fn list_skips_symlinks_named_like_sessions() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new("symlink");
    let store = store(&temp);
    store.save(&sample("abc1")).expect("save");
    let elsewhere = temp.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("folder");
    let target = elsewhere.join("abc-2.toml");
    std::fs::write(&target, session_text(&sample("abc-2"))).expect("target");
    symlink(&target, temp.join("sessions").join("abc-2.toml")).expect("symlink");
    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "abc1");
}

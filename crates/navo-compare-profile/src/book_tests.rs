//! Tests for the recent-session book: profile opening, recording, pruning and loading.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::book::{SessionBook, Visit};
use crate::brand;
use crate::recent::StartRows;
use crate::session::{
    RECENT_KEPT, Rules, SESSION_VERSION, Session, SessionKind, SideKind, SideRef,
};
use crate::session_store::SessionStore;

/// Unix milliseconds used as "now" throughout these tests.
const T: u64 = 1_700_000_000_000;

/// A unique temporary folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty folder named after the test and a counter.
    fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let folder = format!("{}-book-{}-{name}-{count}", brand::SLUG, std::process::id());
        let path = std::env::temp_dir().join(folder);
        std::fs::create_dir(&path).expect("temp folder");
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

/// Builds a valid, unnamed path-pair session with id `id`.
fn pair(id: &str) -> Session {
    let near = std::env::temp_dir();
    Session {
        version: SESSION_VERSION,
        id: id.to_string(),
        name: None,
        kind: SessionKind::Pair,
        left: SideRef {
            kind: SideKind::Path,
            path: near.join("book-left"),
            rev: None,
            url: None,
        },
        right: SideRef {
            kind: SideKind::Path,
            path: near.join("book-right"),
            rev: None,
            url: None,
        },
        rules: Rules {
            whitespace: "keep".to_string(),
            ignore_bom: false,
            ignore_line_endings: false,
            ignore_case: false,
            respect_gitignore: false,
        },
        filter: "all".to_string(),
        opened: 100,
        view: Vec::new(),
    }
}

/// Builds a visit of two path sides under `temp`.
fn visit(temp: &TempDir, left: &str, right: &str, respect_gitignore: bool) -> Visit {
    Visit::paths(temp.join(left), temp.join(right), respect_gitignore)
}

/// Opens a book on `temp` at a fixed `now_s`.
fn open_book(temp: &TempDir) -> SessionBook {
    SessionBook::open(Some(&temp.path), 1_700_000_000).expect("open book")
}

#[test]
fn open_creates_the_profile_and_no_rows() {
    let temp = TempDir::new("open");
    let book = SessionBook::open(Some(&temp.path), 1_700_000_000).expect("open");
    assert!(temp.join("profile.toml").is_file());
    assert!(temp.join("sessions").is_dir());
    assert_eq!(book.rows(), &StartRows::default());
}

#[test]
fn record_saves_one_recent_pair() {
    let temp = TempDir::new("record");
    let mut book = open_book(&temp);
    let visit = visit(&temp, "a", "b", true);
    book.record(&visit, T).expect("record");
    assert_eq!(book.rows().recent.len(), 1);
    assert_eq!(
        book.rows().recent[0].left,
        temp.join("a").to_string_lossy().into_owned()
    );
    assert_eq!(
        book.rows().recent[0].right,
        temp.join("b").to_string_lossy().into_owned()
    );
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.version, SESSION_VERSION);
    assert!(session.name.is_none());
    assert_eq!(session.kind, SessionKind::Pair);
    assert_eq!(session.left.kind, SideKind::Path);
    assert_eq!(session.right.kind, SideKind::Path);
    assert_eq!(session.left.path, temp.join("a"));
    assert_eq!(session.right.path, temp.join("b"));
    assert_eq!(
        session.rules,
        Rules {
            whitespace: "keep".to_string(),
            ignore_bom: false,
            ignore_line_endings: false,
            ignore_case: false,
            respect_gitignore: true,
        }
    );
    assert_eq!(session.filter, "all");
    assert_eq!(session.opened, 1_700_000_000);
    assert!(session.view.is_empty());
}

#[test]
fn record_again_bumps_the_same_session() {
    let temp = TempDir::new("bump");
    let mut book = open_book(&temp);
    let first = visit(&temp, "a", "b", true);
    book.record(&first, T).expect("first record");
    book.record(&first, T + 120_000).expect("second record");
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].opened, 1_700_000_120);
    let other = visit(&temp, "a", "b", false);
    book.record(&other, T + 240_000).expect("other record");
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), 2);
}

#[test]
fn record_again_bumps_one_git_session() {
    let temp = TempDir::new("git-bump");
    let mut book = open_book(&temp);
    let visit = Visit {
        left: SideRef::git(temp.join("repo"), "main".to_string()),
        right: SideRef::path(temp.join("b")),
        respect_gitignore: false,
    };
    book.record(&visit, T).expect("first record");
    book.record(&visit, T + 120_000).expect("second record");
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].opened, 1_700_000_120);
    assert_eq!(
        sessions[0].left,
        SideRef::git(temp.join("repo"), "main".to_string())
    );
}

#[test]
fn record_prunes_past_the_recent_limit() {
    let temp = TempDir::new("prune");
    let mut book = open_book(&temp);
    for i in 0..21u64 {
        let left = format!("l{i:02}");
        let visit = visit(&temp, &left, "r", false);
        book.record(&visit, T + i * 1000).expect("record");
    }
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), RECENT_KEPT);
    assert!(
        sessions
            .iter()
            .all(|session| session.left.path != temp.join("l00"))
    );
    assert_eq!(book.rows().recent.len(), RECENT_KEPT);
}

#[test]
fn record_bumps_a_named_match_and_keeps_its_name() {
    let temp = TempDir::new("named");
    let mut session = pair("a-0");
    session.name = Some("work".to_string());
    session.left.path = temp.join("a");
    session.right.path = temp.join("b");
    session.rules.respect_gitignore = true;
    session.opened = 100;
    SessionStore::open(temp.join("sessions"))
        .save(&session)
        .expect("save named");
    let mut book = open_book(&temp);
    let visit = visit(&temp, "a", "b", true);
    book.record(&visit, T).expect("record");
    let sessions = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "a-0");
    assert_eq!(sessions[0].name.as_deref(), Some("work"));
    assert_eq!(sessions[0].opened, 1_700_000_000);
    assert_eq!(book.rows().named.len(), 1);
    assert!(book.rows().recent.is_empty());
}

#[test]
fn load_gives_the_visit_back() {
    let temp = TempDir::new("load");
    let mut book = open_book(&temp);
    let visit = visit(&temp, "a", "b", false);
    book.record(&visit, T).expect("record");
    let id = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list")[0]
        .id
        .clone();
    assert_eq!(book.load(&id), Ok(visit));
}

#[test]
fn load_returns_git_sides() {
    let temp = TempDir::new("git-load");
    let book = open_book(&temp);
    let mut session = pair("b-0");
    session.left = SideRef::path(temp.join("a"));
    session.right = SideRef::git(temp.join("repo"), "main".to_string());
    SessionStore::open(temp.join("sessions"))
        .save(&session)
        .expect("save git");
    let visit = book.load("b-0").expect("git side");
    assert_eq!(visit.left, SideRef::path(temp.join("a")));
    assert_eq!(
        visit.right,
        SideRef::git(temp.join("repo"), "main".to_string())
    );
}

#[test]
fn load_refuses_missing_files() {
    let temp = TempDir::new("missing-load");
    let book = open_book(&temp);
    let missing = book.load("c-0").expect_err("missing file");
    assert!(missing.starts_with("Could not open this session: "));
    assert!(missing.contains("c-0.toml"));
}

#[test]
fn a_store_failure_becomes_text() {
    let temp = TempDir::new("store-fail");
    let mut book = open_book(&temp);
    std::fs::remove_dir_all(temp.join("sessions")).expect("remove sessions");
    std::fs::write(temp.join("sessions"), b"").expect("file instead");
    let visit = visit(&temp, "a", "b", true);
    let error = book.record(&visit, T).expect_err("record fails");
    assert!(error.starts_with("Could not save this compare to Recent: "));
    assert_eq!(book.rows(), &StartRows::default());
}

#[test]
fn a_newer_profile_is_refused_and_kept() {
    let temp = TempDir::new("newer");
    let marker = temp.join("profile.toml");
    std::fs::write(&marker, "version = 9\n").expect("write marker");
    let error = SessionBook::open(Some(&temp.path), 1_700_000_000)
        .err()
        .expect("newer profile refused");
    assert_eq!(
        error,
        "Recent sessions are off: the profile has version 9; this app reads version 1."
    );
    assert_eq!(
        std::fs::read(&marker).expect("read marker"),
        b"version = 9\n"
    );
}

#[test]
fn forget_removes_one_recent_row() {
    let temp = TempDir::new("forget");
    let mut book = open_book(&temp);
    book.record(&visit(&temp, "a", "b", false), T)
        .expect("record");
    let id = SessionStore::open(temp.join("sessions"))
        .list()
        .expect("list")[0]
        .id
        .clone();
    let session = book.forget(&id, 1_700_000_000).expect("forget");
    assert_eq!(session.id, id);
    assert!(book.rows().recent.is_empty());
    assert!(
        SessionStore::open(temp.join("sessions"))
            .list()
            .expect("list")
            .is_empty()
    );
}

#[test]
fn forget_refuses_a_named_session() {
    let temp = TempDir::new("forget-named");
    let mut book = open_book(&temp);
    let mut named = pair("a-1");
    named.name = Some("Keep me".to_string());
    SessionStore::open(temp.join("sessions"))
        .save(&named)
        .expect("save");
    let error = book.forget("a-1", 1_700_000_000).expect_err("refused");
    assert_eq!(error, "Named sessions are not removed from Recent.");
    assert!(
        SessionStore::open(temp.join("sessions"))
            .load("a-1")
            .is_ok()
    );
}

#[test]
fn restore_brings_the_same_id_back() {
    let temp = TempDir::new("restore");
    let mut book = open_book(&temp);
    book.record(&visit(&temp, "a", "b", false), T)
        .expect("record");
    let id = book.rows().recent[0].id.clone();
    let session = book.forget(&id, 1_700_000_000).expect("forget");
    book.restore(&session, 1_700_000_000).expect("restore");
    assert_eq!(book.rows().recent.len(), 1);
    assert_eq!(book.rows().recent[0].id, id);
}

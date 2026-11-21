//! Tests for the sessions store: NS5, NS9 and the B4 contract.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::brand;
use crate::session::{
    RECENT_KEPT, Rules, SESSION_VERSION, Session, SessionKind, SideKind, SideRef,
};
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
            "{}-session-{}-{name}-{count}",
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

/// Returns an absolute path for tests that never touches the disk.
fn abs(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// Builds a stored side with an absolute path.
fn side(kind: SideKind, name: &str, rev: Option<&str>) -> SideRef {
    SideRef {
        kind,
        path: abs(name),
        rev: rev.map(str::to_string),
        url: None,
    }
}

/// Returns the engine's default rule values.
fn rules() -> Rules {
    Rules {
        whitespace: "keep".to_string(),
        ignore_bom: true,
        ignore_line_endings: false,
        ignore_case: true,
        respect_gitignore: false,
    }
}

/// Builds a valid named-pair session with id `id`.
fn sample(id: &str) -> Session {
    Session {
        version: SESSION_VERSION,
        id: id.to_string(),
        name: None,
        kind: SessionKind::Pair,
        left: side(SideKind::Path, "left", None),
        right: side(SideKind::Git, "repo", Some("main")),
        rules: rules(),
        filter: "all".to_string(),
        opened: 1_700_000_000,
        view: vec!["folder".to_string(), "stream".to_string()],
    }
}

/// Writes `text` as the session file for `id`, creating the folder.
fn write_text(temp: &TempDir, id: &str, text: &str) {
    let dir = temp.join("sessions");
    std::fs::create_dir_all(&dir).expect("sessions folder");
    std::fs::write(dir.join(format!("{id}.toml")), text).expect("session file");
}

/// Reads the bytes of the session file for `id`.
fn read_text(temp: &TempDir, id: &str) -> Vec<u8> {
    std::fs::read(temp.join("sessions").join(format!("{id}.toml"))).expect("session file")
}

/// Serialises `session` as the store would write it.
fn session_text(session: &Session) -> String {
    toml::to_string(session).expect("serialise session")
}

#[test]
fn save_and_load_round_trip() {
    let temp = TempDir::new("round-trip");
    let store = store(&temp);
    let mut session = sample("aaaa");
    session.name = Some("Release notes".to_string());
    store.save(&session).expect("save");
    assert_eq!(store.load("aaaa").expect("load"), session);
    assert!(!temp.join("sessions").join("aaaa.toml.tmp").exists());
}

#[test]
fn invalid_ids_are_refused() {
    let temp = TempDir::new("bad-ids");
    let store = store(&temp);
    let long = "a".repeat(41);
    for id in ["../x", "a/b", "", long.as_str()] {
        let mut session = sample("aaaa");
        session.id = id.to_string();
        assert!(matches!(store.save(&session), Err(StoreError::BadId(_))));
        assert!(matches!(store.load(id), Err(StoreError::BadId(_))));
        assert!(matches!(store.delete(id), Err(StoreError::BadId(_))));
    }
    assert!(store.list().expect("list").is_empty());
}

#[test]
fn invalid_sides_are_refused() {
    let temp = TempDir::new("bad-sides");
    let store = store(&temp);
    let mut relative = sample("aaaa");
    relative.left.path = PathBuf::from("relative");
    assert!(matches!(store.save(&relative), Err(StoreError::BadSide)));
    let mut git_without_rev = sample("aaaa");
    git_without_rev.right.rev = None;
    assert!(matches!(
        store.save(&git_without_rev),
        Err(StoreError::BadSide)
    ));
    let mut path_with_rev = sample("aaaa");
    path_with_rev.left.rev = Some("main".to_string());
    assert!(matches!(
        store.save(&path_with_rev),
        Err(StoreError::BadSide)
    ));
    assert!(store.list().expect("list").is_empty());
}

#[test]
fn invalid_git_revs_are_refused_on_save() {
    let temp = TempDir::new("bad-revs");
    let store = store(&temp);
    let long = "a".repeat(257);
    for rev in ["", "-x", "line\nbreak", long.as_str()] {
        let mut session = sample("aaaa");
        session.right.rev = Some(rev.to_string());
        assert!(
            matches!(store.save(&session), Err(StoreError::BadSide)),
            "rev {rev:?} was accepted"
        );
    }
    assert!(store.list().expect("list").is_empty());
}

#[test]
fn invalid_git_rev_on_disk_is_broken() {
    let temp = TempDir::new("read-rev");
    let store = store(&temp);
    let text = session_text(&sample("aaaa")).replace("rev = \"main\"", "rev = \"-x\"");
    assert!(text.contains("rev = \"-x\""));
    write_text(&temp, "aaaa", &text);
    assert!(matches!(store.load("aaaa"), Err(StoreError::Broken { .. })));
}

#[test]
fn unknown_rule_values_are_refused_on_save() {
    let temp = TempDir::new("bad-rules");
    let store = store(&temp);
    let mut bad_whitespace = sample("aaaa");
    bad_whitespace.rules.whitespace = "sideways".to_string();
    assert!(matches!(
        store.save(&bad_whitespace),
        Err(StoreError::BadSide)
    ));
    let mut bad_filter = sample("aaaa");
    bad_filter.filter = "everything".to_string();
    assert!(matches!(store.save(&bad_filter), Err(StoreError::BadSide)));
}

#[test]
fn texts_are_not_storable() {
    let temp = TempDir::new("texts");
    let store = store(&temp);
    let mut session = sample("aaaa");
    session.kind = SessionKind::Texts;
    assert!(matches!(store.save(&session), Err(StoreError::NotStorable)));
    assert!(!temp.join("sessions").join("aaaa.toml").exists());
}

#[test]
fn newer_file_is_never_overwritten() {
    let temp = TempDir::new("newer");
    let store = store(&temp);
    write_text(&temp, "aaaa", "version = 2\nfuture = true\n");
    let before = read_text(&temp, "aaaa");
    let loaded = store.load("aaaa");
    assert!(matches!(
        loaded,
        Err(StoreError::Newer { ref id, found: 2 }) if id.as_str() == "aaaa"
    ));
    assert!(matches!(
        store.save(&sample("aaaa")),
        Err(StoreError::Newer { found: 2, .. })
    ));
    assert_eq!(read_text(&temp, "aaaa"), before);
}

#[test]
fn broken_files_are_left_untouched() {
    let temp = TempDir::new("broken");
    let store = store(&temp);
    for (id, text) in [
        ("aaaa", "this is not toml [[\n"),
        ("bbbb", "id = \"bbbb\"\n"),
    ] {
        write_text(&temp, id, text);
        let before = read_text(&temp, id);
        assert!(matches!(store.load(id), Err(StoreError::Broken { .. })));
        assert!(matches!(
            store.save(&sample(id)),
            Err(StoreError::Broken { .. })
        ));
        assert_eq!(read_text(&temp, id), before);
    }
}

#[test]
fn inner_id_must_match_file_name() {
    let temp = TempDir::new("inner-id");
    let store = store(&temp);
    write_text(&temp, "bbbb", &session_text(&sample("aaaa")));
    assert!(matches!(store.load("bbbb"), Err(StoreError::Broken { .. })));
    assert!(store.list().expect("list").is_empty());
}

#[test]
fn unknown_rule_values_are_broken_on_read() {
    let temp = TempDir::new("read-rules");
    let store = store(&temp);
    let base = session_text(&sample("aaaa"));
    let whitespace = base.replace("whitespace = \"keep\"", "whitespace = \"sideways\"");
    assert_ne!(whitespace, base);
    write_text(&temp, "aaaa", &whitespace);
    assert!(matches!(store.load("aaaa"), Err(StoreError::Broken { .. })));
    let filter =
        session_text(&sample("bbbb")).replace("filter = \"all\"", "filter = \"everything\"");
    assert!(filter.contains("everything"));
    write_text(&temp, "bbbb", &filter);
    assert!(matches!(store.load("bbbb"), Err(StoreError::Broken { .. })));
}

#[test]
fn leftover_temp_files_are_ignored() {
    let temp = TempDir::new("temp-file");
    let store = store(&temp);
    store.save(&sample("aaaa")).expect("save");
    let sessions = temp.join("sessions");
    let pid = std::process::id();
    for name in [format!("aaaa.toml.{pid}.tmp"), "aaaa.toml.tmp".to_string()] {
        std::fs::write(sessions.join(name), "version = 2\n").expect("temp file");
    }
    assert_eq!(store.load("aaaa").expect("load"), sample("aaaa"));
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn missing_folder_is_created_by_save() {
    let temp = TempDir::new("missing");
    let store = store(&temp);
    assert!(store.list().expect("list").is_empty());
    store.save(&sample("aaaa")).expect("save");
    assert!(temp.join("sessions").is_dir());
    assert!(store.load("aaaa").is_ok());
}

#[test]
fn prune_keeps_twenty_unnamed_and_all_named() {
    let temp = TempDir::new("prune");
    let store = store(&temp);
    for index in 1..=25u32 {
        let mut session = sample(&format!("a{index:x}"));
        session.opened = i64::from(index);
        store.save(&session).expect("save unnamed");
    }
    for index in 1..=3u32 {
        let mut session = sample(&format!("b{index}"));
        session.name = Some(format!("Named {index}"));
        session.opened = 0;
        store.save(&session).expect("save named");
    }
    assert_eq!(store.prune_recent().expect("prune"), 5);
    let listed = store.list().expect("list");
    assert_eq!(listed.len(), 23);
    let unnamed = listed.iter().filter(|item| item.name.is_none()).count();
    assert_eq!(unnamed, RECENT_KEPT);
    assert_eq!(listed.iter().filter(|item| item.name.is_some()).count(), 3);
    for index in 1..=3u32 {
        assert!(store.load(&format!("b{index}")).is_ok());
    }
    for index in 1..=5u32 {
        let gone = format!("a{index:x}");
        assert!(!listed.iter().any(|item| item.id == gone));
    }
}

#[test]
fn list_is_newest_first_with_id_breaks_ties() {
    let temp = TempDir::new("order");
    let store = store(&temp);
    for (id, opened) in [("aaaa", 5), ("bbbb", 7), ("cccc", 6), ("dddd", 7)] {
        let mut session = sample(id);
        session.opened = opened;
        store.save(&session).expect("save");
    }
    let ids: Vec<String> = store
        .list()
        .expect("list")
        .into_iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(ids, ["bbbb", "dddd", "cccc", "aaaa"]);
}

#[test]
fn new_id_is_time_hex_and_unique_in_the_folder() {
    let temp = TempDir::new("new-id");
    let store = store(&temp);
    let now = 1_700_000_000_000u64;
    let first = store.new_id(now);
    assert!(first.starts_with(&format!("{now:x}-")));
    store.save(&sample(&first)).expect("save");
    let second = store.new_id(now);
    assert_ne!(first, second);
    assert!(store.save(&sample(&second)).is_ok());
}

#[test]
fn delete_removes_the_file_and_repeats_cleanly() {
    let temp = TempDir::new("delete");
    let store = store(&temp);
    store.save(&sample("aaaa")).expect("save");
    store.delete("aaaa").expect("first delete");
    assert!(matches!(store.load("aaaa"), Err(StoreError::Io { .. })));
    store.delete("aaaa").expect("second delete");
    assert!(matches!(store.delete("../x"), Err(StoreError::BadId(_))));
}

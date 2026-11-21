//! Tests for the start rows built from stored sessions (NS13).

use std::path::PathBuf;

use crate::recent::{RowKind, age_text, side_text, start_rows};
use crate::session::{Rules, SESSION_VERSION, Session, SessionKind, SideKind, SideRef};

/// Builds a side with an absolute path and an optional Git rev.
fn side(kind: SideKind, path: &str, rev: Option<&str>) -> SideRef {
    SideRef {
        kind,
        path: PathBuf::from(path),
        rev: rev.map(str::to_string),
        url: None,
    }
}

/// Builds a path side.
fn path(text: &str) -> SideRef {
    side(SideKind::Path, text, None)
}

/// Builds a Git side.
fn git(text: &str, rev: Option<&str>) -> SideRef {
    side(SideKind::Git, text, rev)
}

/// Builds a pair session; `opened` is in Unix seconds.
#[expect(clippy::too_many_arguments, reason = "test fixture")]
fn session(id: &str, name: Option<&str>, left: SideRef, right: SideRef, opened: i64) -> Session {
    Session {
        version: SESSION_VERSION,
        id: id.to_string(),
        name: name.map(str::to_string),
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
        opened,
        view: Vec::new(),
    }
}

#[test]
fn side_text_shows_a_path_or_path_at_rev() {
    assert_eq!(side_text(&path("/data/left")), "/data/left");
    assert_eq!(side_text(&git("/repo", Some("main"))), "/repo@main");
    assert_eq!(side_text(&git("/repo", None)), "/repo");
}

#[cfg(unix)]
#[test]
fn side_text_replaces_invalid_bytes() {
    use std::os::unix::ffi::OsStrExt;
    let bytes = std::ffi::OsStr::from_bytes(b"/data/\xff");
    let weird = SideRef {
        kind: SideKind::Path,
        path: PathBuf::from(bytes),
        rev: None,
        url: None,
    };
    assert_eq!(side_text(&weird), "/data/\u{FFFD}");
}

#[test]
fn age_text_uses_the_thresholds() {
    const DAY: i64 = 86_400;
    let now = 1_000_000_000;
    for (seconds, expected) in [
        (0, "now"),
        (59, "now"),
        (60, "1 min ago"),
        (3_599, "59 min ago"),
        (3_600, "1 h ago"),
        (86_399, "23 h ago"),
        (86_400, "1 d ago"),
        (29 * DAY, "29 d ago"),
        (30 * DAY, "1 mo ago"),
        (364 * DAY, "12 mo ago"),
        (365 * DAY, "1 y ago"),
    ] {
        assert_eq!(age_text(now - seconds, now), expected, "{seconds} seconds");
    }
}

#[test]
fn age_text_treats_a_future_open_as_now() {
    assert_eq!(age_text(1_000_005, 1_000_000), "now");
}

#[test]
fn age_text_handles_extreme_open_times() {
    assert_eq!(age_text(i64::MAX, 0), "now");
    let text = age_text(i64::MIN, 0);
    assert!(text.ends_with(" y ago"), "{text}");
}

#[test]
fn start_rows_splits_names_and_keeps_recent_order() {
    let sessions = [
        session("aaaa", None, path("/a"), git("/b", Some("main")), 10),
        session("bbbb", Some("Zed"), path("/c"), path("/d"), 20),
        session("cccc", None, git("/e", None), git("/f", Some("dev")), 30),
        session("dddd", Some("alpha"), path("/g"), path("/h"), 40),
        session("eeee", Some("Alpha"), path("/i"), path("/j"), 50),
    ];
    let rows = start_rows(&sessions, 100);
    let recent: Vec<&str> = rows.recent.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(recent, ["aaaa", "cccc"]);
    let named: Vec<&str> = rows.named.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(named, ["dddd", "eeee", "bbbb"]);
    assert_eq!(rows.recent[0].left, "/a");
    assert_eq!(rows.recent[0].right, "/b@main");
    assert_eq!(rows.recent[0].kind, RowKind::Mixed);
    assert_eq!(rows.recent[0].age, "1 min ago");
    assert_eq!(rows.recent[1].kind, RowKind::GitRefs);
    assert_eq!(rows.named[0].kind, RowKind::Paths);
    assert_eq!(rows.named[0].name.as_deref(), Some("alpha"));
}

#[test]
fn blank_names_become_recent_rows() {
    let sessions = [session("aaaa", Some("   "), path("/l"), path("/r"), 5)];
    let rows = start_rows(&sessions, 10);
    assert!(rows.named.is_empty());
    assert_eq!(rows.recent.len(), 1);
    assert_eq!(rows.recent[0].name, None);
    assert_eq!(rows.recent[0].kind, RowKind::Paths);
    assert_eq!(rows.recent[0].age, "now");
}

#[test]
fn texts_sessions_are_skipped() {
    let mut texts = session("aaaa", Some("Kept"), path("/l"), path("/r"), 5);
    texts.kind = SessionKind::Texts;
    let rows = start_rows(&[texts], 10);
    assert!(rows.recent.is_empty());
    assert!(rows.named.is_empty());
}

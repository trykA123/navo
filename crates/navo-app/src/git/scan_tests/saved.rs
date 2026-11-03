use navo_store::CacheDb;

use super::super::ScanError;
use super::super::listing::project;
use super::*;

const DAY: i64 = 24 * 60 * 60;

fn sample() -> Cached {
    let mut cached = project(Err(ScanError::NoGit), &[], Path::new("work/repo"));
    cached.root.kind = BadgeKind::Diverged;
    cached.root.reason = None;
    cached.root.branch = Some("main".to_owned());
    cached.root.upstream = Some("origin/main".to_owned());
    (cached.root.ahead, cached.root.behind, cached.root.changed) = (2, 3, 4);
    cached.root.fetched_ms = Some(1_700_000_000_000);
    cached.dirs.insert(
        "src".to_owned(),
        (BadgeKind::Unknown, Some(BadgeReason::TimedOut)),
    );
    cached
}

#[test]
fn an_entry_comes_back_as_it_was_saved_and_stale() {
    let bytes = encode(&sample(), 1_000).unwrap();
    let back = decode(&bytes, 1_000 + DAY).unwrap();
    assert!(back.stale && back.root.stale && back.root.is_root);
    assert_eq!(back.root_name, "repo");
    assert_eq!(back.root.kind, BadgeKind::Diverged);
    assert_eq!(back.root.branch.as_deref(), Some("main"));
    assert_eq!(back.root.upstream.as_deref(), Some("origin/main"));
    assert_eq!(
        (back.root.ahead, back.root.behind, back.root.changed),
        (2, 3, 4)
    );
    assert_eq!(back.root.fetched_ms, Some(1_700_000_000_000));
    assert_eq!(
        back.dirs["src"],
        (BadgeKind::Unknown, Some(BadgeReason::TimedOut))
    );
}

#[test]
fn entries_older_than_thirty_days_unknown_versions_and_garbage_are_dropped() {
    let bytes = encode(&sample(), 1_000).unwrap();
    assert!(decode(&bytes, 1_000 + 30 * DAY).is_some());
    assert!(decode(&bytes, 1_000 + 30 * DAY + 1).is_none());
    let text = String::from_utf8(bytes).unwrap();
    let future = text.replace("\"version\":1", "\"version\":2");
    assert_ne!(future, text);
    assert!(decode(future.as_bytes(), 1_000).is_none());
    assert!(decode(b"not json", 1_000).is_none());
    let renamed = text.replace("\"diverged\"", "\"sideways\"");
    assert!(decode(renamed.as_bytes(), 1_000).is_none());
}

#[test]
fn loading_removes_the_entries_it_cannot_use() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let now = now_secs();
    {
        let (mut db, _) = CacheDb::open(&path).unwrap();
        db.put_git_badge("work/fresh", &encode(&sample(), now).unwrap())
            .unwrap();
        db.put_git_badge("work/old", &encode(&sample(), now - 31 * DAY).unwrap())
            .unwrap();
        db.put_git_badge("work/broken", b"{").unwrap();
    }
    let saved = DbSaved::at(path.clone());
    let loaded = saved.load();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0, PathBuf::from("work/fresh"));
    let (db, _) = CacheDb::open(&path).unwrap();
    assert_eq!(db.git_badges().unwrap().len(), 1);
}

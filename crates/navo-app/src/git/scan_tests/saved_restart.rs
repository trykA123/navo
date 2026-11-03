use super::*;
use navo_ui::git::BadgeKind;

fn store(dir: &tempfile::TempDir) -> Arc<dyn SavedBadges> {
    Arc::new(DbSaved::at(dir.path().join("cache.db")))
}

fn modified() -> RepoStatus {
    status_with(b"? new.txt\0")
}

#[test]
fn a_new_process_shows_the_saved_status_as_stale_until_the_refresh_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let first = Harness::with_saved(probe(&["fixture/repo"]), store(&dir));
    first.observe("fixture", 1, &["repo"]);
    first.next();
    first.reply(Ok(modified()));
    first.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| !b.stale)
    });
    assert!(first.scan.stop());

    let second = Harness::with_saved(probe(&["fixture/repo"]), store(&dir));
    second.observe("fixture", 1, &["repo"]);
    let stale = second.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| b.stale)
    });
    assert_eq!(stale.by_name["repo"].kind, BadgeKind::Modified);
    assert_eq!(stale.by_name["repo"].branch.as_deref(), Some("main"));
    second.next();
    second.reply(Ok(clean()));
    let fresh = second.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| !b.stale)
    });
    assert_eq!(fresh.by_name["repo"].kind, BadgeKind::Clean);
    assert!(second.scan.stop());
}

#[test]
fn a_restarted_scanner_over_the_same_store_keeps_the_badges() {
    let dir = tempfile::tempdir().unwrap();
    let saved = store(&dir);
    let first = Harness::with_saved(probe(&["fixture/repo"]), saved.clone());
    first.observe("fixture", 1, &["repo"]);
    first.next();
    first.reply(Ok(modified()));
    first.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| !b.stale)
    });
    drop(first);

    let second = Harness::with_saved(probe(&["fixture/repo"]), saved);
    second.observe("fixture", 2, &["repo"]);
    let stale = second.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| b.stale)
    });
    assert_eq!(stale.by_name["repo"].kind, BadgeKind::Modified);
    assert!(second.scan.stop());
}

#[test]
fn an_unreadable_store_does_not_stop_scanning() {
    let dir = tempfile::tempdir().unwrap();
    let broken: Arc<dyn SavedBadges> = Arc::new(DbSaved::at(dir.path().to_path_buf()));
    let h = Harness::with_saved(probe(&["fixture/repo"]), broken);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(clean()));
    let badges = h.wait("fixture", |b| {
        b.by_name.get("repo").is_some_and(|b| !b.stale)
    });
    assert_eq!(badges.by_name["repo"].kind, BadgeKind::Clean);
    assert!(h.scan.stop());
}

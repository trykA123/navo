use super::*;
use navo_git::trust::Refusal;
use navo_ui::git::BadgeReason;

#[test]
fn source_errors_publish_unknown_with_their_reason_and_are_not_cached() {
    let h = Harness::new(probe(&["fixture/a"]));
    h.observe("fixture", 1, &["a"]);
    h.next();
    h.reply(Err(ScanError::Refused(Refusal::Guard)));
    let badges = h.wait("fixture", |b| b.by_name.get("a").is_some_and(|b| !b.stale));
    assert_eq!(badges.by_name["a"].kind, BadgeKind::Unknown);
    assert!(badges.by_name["a"].is_root);
    assert_eq!(badges.by_name["a"].reason, Some(BadgeReason::Guard));
    h.scan.rescan(&location("fixture"));
    h.next();
    let shown = h.scan.badges(&location("fixture")).unwrap();
    assert!(
        !shown.by_name["a"].stale,
        "the error is not republished as cached"
    );
    h.reply(Ok(clean()));
    let fresh = h.wait("fixture", |b| {
        b.by_name
            .get("a")
            .is_some_and(|b| b.kind == BadgeKind::Clean)
    });
    assert!(fresh.revision > shown.revision);
    assert!(h.scan.stop());
}

#[test]
fn folders_inside_a_repository_get_subtree_badges_without_repository_counts() {
    let h = Harness::new(probe(&["fixture/repo"]));
    h.observe("fixture/repo/src", 3, &["deep", "other"]);
    assert_eq!(h.next().0, PathBuf::from("fixture/repo"));
    h.reply(Ok(status_with(b"? src/deep/file\0")));
    let badges = h.wait("fixture/repo/src", |b| {
        b.by_name.len() == 2 && b.by_name.values().all(|b| !b.stale)
    });
    assert_eq!(badges.by_name["deep"].kind, BadgeKind::Modified);
    assert_eq!(badges.by_name["other"].kind, BadgeKind::Clean);
    assert_eq!(badges.by_name["deep"].untracked, 0);
    assert!(!badges.by_name["deep"].is_root);
    assert_eq!(badges.by_name["deep"].branch.as_deref(), Some("main"));
    let root = &badges.roots["deep"];
    assert_eq!(root.name, "repo");
    assert!(root.badge.is_root);
    assert_eq!(root.badge.kind, BadgeKind::Modified);
    assert_eq!(root.badge.untracked, 1);
    assert!(h.scan.stop());
}

#[test]
fn an_empty_folder_discovers_its_parent_repository() {
    let h = Harness::new(probe(&["fixture/repo"]));
    h.observe("fixture/repo/empty", 1, &[]);
    assert_eq!(h.next().0, PathBuf::from("fixture/repo"));
    h.reply(Ok(status_with(b"? elsewhere.txt\0")));
    let badges = h.wait("fixture/repo/empty", |b| b.repository.is_some());
    assert!(badges.by_name.is_empty());
    assert!(badges.roots.is_empty());
    let repository = badges.repository.as_ref().unwrap();
    assert_eq!(repository.name, "repo");
    assert_eq!(repository.badge.branch.as_deref(), Some("main"));
    assert_eq!(repository.badge.untracked, 1);
    h.observe("fixture/outside", 2, &[]);
    let outside = h.wait("fixture/outside", |b| b.revision >= 2);
    assert!(outside.repository.is_none());
    assert!(h.calls.try_recv().is_err());
    assert!(h.scan.stop());
}

#[test]
fn capped_status_makes_subfolders_unknown() {
    let h = Harness::new(probe(&["fixture/repo"]));
    h.observe("fixture/repo", 1, &["src"]);
    h.next();
    let mut status = clean();
    status.capped = true;
    h.reply(Ok(status));
    let badges = h.wait("fixture/repo", |b| {
        b.by_name.get("src").is_some_and(|b| !b.stale)
    });
    assert_eq!(badges.by_name["src"].kind, BadgeKind::Unknown);
    assert!(h.scan.stop());
}

#[test]
fn stale_publication_omits_folders_the_cached_status_never_covered() {
    let h = Harness::new(probe(&["fixture/repo"]));
    h.observe("fixture/repo/src", 1, &["deep"]);
    h.next();
    h.reply(Ok(clean()));
    h.wait("fixture/repo/src", |b| b.by_name.contains_key("deep"));
    h.observe("fixture/repo/docs", 2, &["guide"]);
    h.next();
    assert!(
        h.scan
            .badges(&location("fixture/repo/docs"))
            .is_some_and(|b| b.by_name.is_empty())
    );
    h.reply(Ok(clean()));
    h.wait("fixture/repo/docs", |b| {
        b.by_name.get("guide").is_some_and(|b| !b.stale)
    });
    assert!(h.scan.stop());
}

#[test]
fn stop_gives_up_within_two_seconds_on_a_source_that_ignores_cancellation() {
    let h = Harness::new(probe(&["fixture/a"]));
    h.source.ignore_cancel.store(true, Ordering::SeqCst);
    h.observe("fixture", 1, &["a"]);
    h.next();
    let began = Instant::now();
    assert!(!h.scan.stop());
    assert!(began.elapsed() < Duration::from_millis(2500));
    h.source.ignore_cancel.store(false, Ordering::SeqCst);
}

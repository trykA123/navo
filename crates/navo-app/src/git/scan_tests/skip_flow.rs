use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use navo_fs::DriveClass;
use navo_ui::git::{BadgeKind, BadgeReason};

use super::super::ScanError;
use super::super::tests::{clean, location, status_with};
use skip_probe::{SkipProbe, fresh, harness, prints, repo, scanned};

#[path = "skip_expiry.rs"]
mod expiry;
#[path = "skip_leave.rs"]
mod leave;
#[path = "skip_probe.rs"]
pub(crate) mod skip_probe;
#[path = "skip_upstream.rs"]
mod upstream;

#[test]
fn an_unchanged_reopen_is_shown_stale_then_fresh_without_a_status_call() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    let stale = h.wait("fixture", |badges| badges.by_name["repo"].stale);
    assert_eq!(stale.by_name["repo"].kind, BadgeKind::Clean);
    let shown = fresh(&h, "fixture", "repo");
    assert!(shown.revision >= 2);
    assert!(
        h.calls.try_recv().is_err(),
        "an unchanged reopen must not call"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_change_before_the_confirm_deadline_calls() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    probe.bump(&repo());
    assert_eq!(h.next().0, repo(), "a change inside the window scans");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");
    assert!(h.scan.stop());
}

#[test]
fn a_change_during_the_status_call_calls_on_the_next_open() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    probe.bump(&repo());
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "the scan saw a change it cannot cover");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_change_before_the_reopen_calls() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    probe.bump(&repo());
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo());
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_fingerprint_change_calls() {
    let (print, changed) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    probe.set_print(Some(changed));
    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    assert_eq!(h.next().0, repo(), "the .git fingerprint changed");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_fingerprint_change_during_the_call_leaves_no_proof() {
    let (print, changed) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    probe.set_print(Some(changed));
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "the two fingerprints disagree");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_proof_under_five_minutes_reuses() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    probe.advance(Duration::from_secs(290));
    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    fresh(&h, "fixture", "repo");
    assert!(
        h.calls.try_recv().is_err(),
        "under five minutes is still fresh"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_proof_at_five_minutes_calls() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    probe.advance(Duration::from_secs(300));
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "5 minutes is stale");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_rescan_calls_and_a_changed_rescan_reuses() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    let before = h.revision("fixture");
    h.scan.rescan(&location("fixture"));
    assert_eq!(h.next().0, repo(), "Recheck always calls");
    h.reply(Ok(clean()));
    h.wait("fixture", |badges| {
        badges.revision > before && !badges.by_name["repo"].stale
    });

    h.scan.rescan_changed(&location("fixture"));
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    fresh(&h, "fixture", "repo");
    assert!(
        h.calls.try_recv().is_err(),
        "a changed rescan with no change reuses"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_repository_rescan_calls_only_that_root_and_an_unknown_path_calls_all() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/a", "fixture/b"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["a", "b"]);
    let mut roots = [h.next().0, h.next().0];
    roots.sort();
    assert_eq!(
        roots,
        [PathBuf::from("fixture/a"), PathBuf::from("fixture/b")]
    );
    h.reply(Ok(clean()));
    h.reply(Ok(clean()));
    h.wait("fixture", |badges| {
        badges.by_name.len() == 2 && badges.by_name.values().all(|badge| !badge.stale)
    });

    let before = h.revision("fixture");
    h.scan
        .view()
        .rescan_repo(&location("fixture"), PathBuf::from("fixture/b"));
    assert_eq!(h.next().0, PathBuf::from("fixture/b"));
    assert!(
        h.calls.try_recv().is_err(),
        "only the named repository scans"
    );
    h.reply(Ok(clean()));
    h.wait("fixture", |badges| {
        badges.revision > before && !badges.by_name["b"].stale
    });

    h.scan
        .view()
        .rescan_repo(&location("fixture"), PathBuf::from("fixture/missing"));
    let mut roots = [h.next().0, h.next().0];
    roots.sort();
    assert_eq!(
        roots,
        [PathBuf::from("fixture/a"), PathBuf::from("fixture/b")],
        "an unmatched path forces every root"
    );
    h.reply(Ok(clean()));
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn subfolder_rows_reuse_states_from_the_kept_status() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture/repo/src", 1, &["deep"]);
    assert_eq!(h.next().0, repo());
    h.reply(Ok(status_with(b"? src/deep/file\0")));
    let first = fresh(&h, "fixture/repo/src", "deep");
    assert_eq!(first.by_name["deep"].kind, BadgeKind::Modified);

    h.observe("fixture/repo/src", 2, &["deep"]);
    h.wait("fixture/repo/src", |badges| badges.by_name["deep"].stale);
    let shown = fresh(&h, "fixture/repo/src", "deep");
    assert_eq!(shown.by_name["deep"].kind, BadgeKind::Modified);
    assert!(h.calls.try_recv().is_err());
    assert!(h.scan.stop());
}

#[test]
fn an_error_result_calls_next_time() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Err(ScanError::Failed));
    h.wait("fixture", |badges| {
        badges
            .by_name
            .get("repo")
            .is_some_and(|badge| badge.reason == Some(BadgeReason::Failed))
    });

    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "an error leaves no proof");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn network_always_calls() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Network, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), true);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "a network drive is never skipped");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_truncated_listing_calls_every_root() {
    let (print, _) = prints();
    let repos: Vec<String> = (0..65)
        .map(|index| format!("fixture/repo-{index}"))
        .collect();
    let names: Vec<String> = (0..65).map(|index| format!("repo-{index}")).collect();
    let all: Vec<&str> = names.iter().map(String::as_str).collect();
    let listed: Vec<&str> = names[..64].iter().map(String::as_str).collect();
    let probe = Arc::new(SkipProbe::new(
        DriveClass::Ssd,
        &repos.iter().map(String::as_str).collect::<Vec<_>>(),
    ));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);

    h.observe("fixture", 1, &listed);
    for _ in 0..64 {
        h.next();
        h.reply(Ok(clean()));
    }
    let all_fresh = |badges: &navo_ui::git::FolderBadges| {
        badges.by_name.len() == 64 && badges.by_name.values().all(|badge| !badge.stale)
    };
    h.wait("fixture", all_fresh);

    h.observe("fixture", 2, &listed);
    h.wait("fixture", |badges| {
        badges.by_name.len() == 64 && badges.by_name.values().all(|badge| badge.stale)
    });
    h.wait("fixture", all_fresh);
    assert!(
        h.calls.try_recv().is_err(),
        "64 repositories still reuse their proofs"
    );

    let before = h.revision("fixture");
    h.observe("fixture", 3, &all);
    for _ in 0..64 {
        h.next();
        h.reply(Ok(clean()));
    }
    assert!(
        h.calls.try_recv().is_err(),
        "truncation scans every root discovery kept"
    );
    h.wait("fixture", |badges| {
        badges.revision > before && all_fresh(badges)
    });
    assert!(h.scan.stop());
}

#[test]
fn a_forget_before_the_confirm_drops_it() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    h.scan.forget(&location("fixture"));
    h.wait_parked();
    assert!(
        h.calls.try_recv().is_err(),
        "a forgotten confirm is dropped"
    );
    assert!(h.scan.badges(&location("fixture")).is_none());
    assert!(h.scan.stop());
}

#[test]
fn a_newer_observe_before_the_confirm_drops_it() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    probe.bump(&repo());
    h.observe("fixture", 3, &["repo"]);
    assert_eq!(h.next().0, repo());
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");
    h.wait_parked();
    assert!(
        h.calls.try_recv().is_err(),
        "the superseded listing's confirm is dropped"
    );
    assert!(h.scan.stop());
}

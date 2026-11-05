use std::sync::Arc;
use std::time::Duration;

use navo_fs::DriveClass;
use navo_ui::git::BadgeKind;

use super::skip_probe::{SkipProbe, fresh, harness, prints, repo, scanned};
use crate::git::scan::tests::{Harness, clean, location};

/// Moves the fake clock and wakes a scanner that is waiting on its next deadline.
fn advance(h: &Harness, probe: &SkipProbe, by: Duration) {
    probe.advance(by);
    wake(h);
}

fn wake(h: &Harness) {
    let _shared = h.scan.shared.0.lock().unwrap();
    h.scan.shared.1.notify_one();
}

#[test]
fn a_scanned_root_without_a_proof_dims_after_five_minutes() {
    let probe = Arc::new(SkipProbe::new(DriveClass::Removable, &["fixture/repo"]));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    assert_eq!(h.next().0, repo());
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    advance(&h, &probe, Duration::from_secs(300));
    let dimmed = h.wait("fixture", |badges| badges.by_name["repo"].stale);
    assert_eq!(dimmed.by_name["repo"].kind, BadgeKind::Clean);
    assert!(
        h.calls.try_recv().is_err(),
        "dimming a badge never calls Git"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_reused_root_dims_five_minutes_after_its_check() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    advance(&h, &probe, Duration::from_secs(280));
    h.observe("fixture", 2, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    fresh(&h, "fixture", "repo");

    advance(&h, &probe, Duration::from_secs(20));
    let dimmed = h.wait("fixture", |badges| badges.by_name["repo"].stale);
    assert_eq!(dimmed.by_name["repo"].kind, BadgeKind::Clean);
    assert!(
        h.calls.try_recv().is_err(),
        "a reused root dims without a status call"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_processed_expiry_is_never_published_twice() {
    let probe = Arc::new(SkipProbe::new(DriveClass::Removable, &["fixture/repo"]));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    advance(&h, &probe, Duration::from_secs(300));
    let revision = h
        .wait("fixture", |badges| badges.by_name["repo"].stale)
        .revision;
    h.wait_parked();
    while h.notices.try_recv().is_ok() {}
    wake(&h);
    assert!(
        h.notices.recv_timeout(Duration::from_millis(100)).is_err(),
        "a processed deadline is removed"
    );
    assert_eq!(h.revision("fixture"), revision);
    assert!(h.scan.stop());
}

#[test]
fn a_forgotten_listing_gets_no_expiry() {
    let probe = Arc::new(SkipProbe::new(DriveClass::Removable, &["fixture/repo"]));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.scan.forget(&location("fixture"));
    h.wait_parked();
    while h.notices.try_recv().is_ok() {}
    advance(&h, &probe, Duration::from_secs(300));
    assert!(h.scan.badges(&location("fixture")).is_none());
    assert!(
        h.notices.recv_timeout(Duration::from_millis(100)).is_err(),
        "a forgotten listing gets no expiry publish"
    );
    assert!(h.scan.stop());
}

//! Scanner tests for a status paused by a file operation: the badge stays and the root retries
//! with a growing delay.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_fs::DriveClass;
use navo_ui::git::BadgeKind;

use super::*;

use crate::git::scan::plan::skip_probe::{SkipProbe, fresh, harness, prints, repo, scanned};

/// Waits until `ready` holds, failing after three seconds.
fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(Instant::now() < deadline, "the condition never held");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A settled fixed drive with one clean repository and a kept proof.
fn settled() -> (Arc<SkipProbe>, Harness) {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");
    (probe, h)
}

/// A settled fixed drive with two clean repositories and kept proofs.
fn settled_two() -> (Arc<SkipProbe>, Harness) {
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
    (probe, h)
}

/// Forces one root and answers its status with `Paused`, then waits for the scanner to park.
fn pause_status(h: &Harness) {
    h.scan.view().rescan_repo(&location("fixture"), repo());
    assert_eq!(h.next().0, repo());
    h.reply(Err(ScanError::Paused));
    h.wait_parked();
}

/// Advances most of `wait`, checks no retry came early, then completes the wait.
fn retry_after(h: &Harness, probe: &SkipProbe, wait: Duration) {
    advance_and_wake(h, probe, wait - Duration::from_millis(100));
    assert!(
        h.calls.recv_timeout(Duration::from_millis(25)).is_err(),
        "a retry came before {wait:?}"
    );
    advance_and_wake(h, probe, Duration::from_millis(100));
    assert_eq!(h.next().0, repo());
}

/// Moves the fake clock and wakes the parked scanner.
fn advance_and_wake(h: &Harness, probe: &SkipProbe, by: Duration) {
    probe.advance(by);
    h.scan.shared.1.notify_one();
}

/// The root keys the listing still marks as forced.
fn forcing(h: &Harness) -> Vec<String> {
    let shared = h.scan.shared.0.lock().unwrap();
    shared
        .active
        .get(&location("fixture"))
        .map(|entry| entry.forcing.clone())
        .unwrap_or_default()
}

#[test]
fn a_paused_status_keeps_the_last_badge_without_a_failure_reason() {
    let (_probe, h) = settled();
    pause_status(&h);
    let badges = h.scan.badges(&location("fixture")).unwrap();
    assert_eq!(badges.by_name["repo"].kind, BadgeKind::Clean);
    assert_eq!(badges.by_name["repo"].reason, None);
    assert!(!badges.by_name["repo"].stale);
    assert!(h.scan.stop());
}

#[test]
fn a_paused_forced_root_leaves_the_forcing_set() {
    let (_probe, h) = settled();
    pause_status(&h);
    assert!(
        forcing(&h).is_empty(),
        "the paused root is no longer listed as forced"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_paused_status_drops_the_proof_before_the_retry() {
    let (_probe, h) = settled();
    pause_status(&h);
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(
        h.next().0,
        repo(),
        "the unforced reopen runs a status instead of reusing the proof"
    );
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_retry_leaves_another_root_status_running() {
    let (probe, h) = settled_two();
    h.scan.view().rescan(&location("fixture"));
    let (first, first_cancel) = h.next();
    let (_second, second_cancel) = h.next();
    h.reply(Err(ScanError::Paused));
    wait_until(|| forcing(&h).len() < 2);
    advance_and_wake(&h, &probe, Duration::from_millis(500));
    let (retried, _) = h.next();
    let other = if retried == first {
        &second_cancel
    } else {
        &first_cancel
    };
    assert!(
        !other.is_cancelled(),
        "a retry must not cancel another root's status"
    );
    assert!(h.scan.stop());
}

#[test]
fn a_paused_status_backs_off_to_four_seconds() {
    let (probe, h) = settled();
    pause_status(&h);
    retry_after(&h, &probe, Duration::from_millis(500));
    h.reply(Err(ScanError::Paused));
    h.wait_parked();
    for wait in [
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(4),
    ] {
        retry_after(&h, &probe, wait);
        h.reply(Err(ScanError::Paused));
        h.wait_parked();
    }
    retry_after(&h, &probe, Duration::from_secs(4));
    let before = h.revision("fixture");
    h.reply(Ok(clean()));
    h.wait("fixture", |badges| badges.revision > before);
    pause_status(&h);
    retry_after(&h, &probe, Duration::from_millis(500));
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_paused_status_retries_after_500_ms_and_shows_clean() {
    let (probe, h) = settled();
    let before = h.revision("fixture");
    pause_status(&h);
    advance_and_wake(&h, &probe, Duration::from_millis(400));
    assert!(
        h.calls.recv_timeout(Duration::from_millis(50)).is_err(),
        "no retry before 500 ms"
    );
    advance_and_wake(&h, &probe, Duration::from_millis(100));
    assert_eq!(h.next().0, repo());
    h.reply(Ok(clean()));
    let badges = h.wait("fixture", |badges| {
        badges.revision > before && badges.by_name["repo"].kind == BadgeKind::Clean
    });
    assert_eq!(badges.by_name["repo"].reason, None);
    assert!(h.scan.stop());
}

#[test]
fn a_forgotten_listing_gets_no_retry() {
    let (probe, h) = settled();
    pause_status(&h);
    let before = h.revision("fixture");
    h.scan.forget(&location("fixture"));
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "the new listing scans normally");
    h.reply(Ok(clean()));
    wait_until(|| h.revision("fixture") > before);
    advance_and_wake(&h, &probe, Duration::from_millis(500));
    assert!(
        h.calls.recv_timeout(Duration::from_millis(200)).is_err(),
        "the forgotten listing must not retry"
    );
    assert!(h.scan.stop());
}

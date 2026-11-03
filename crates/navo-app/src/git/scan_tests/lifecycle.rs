use super::*;
use navo_ui::git::BadgeReason;

#[test]
fn forget_cancels_running_calls_and_discards_late_results() {
    let h = Harness::new(probe(&["fixture/a", "fixture/b", "fixture/c"]));
    h.observe("fixture", 1, &["a", "b", "c"]);
    let first = h.next();
    let second = h.next();
    h.scan.forget(&location("fixture"));
    assert!(first.1.is_cancelled());
    assert!(second.1.is_cancelled());
    assert!(h.scan.badges(&location("fixture")).is_none());
    assert!(h.scan.stop());
    assert!(h.calls.try_iter().next().is_none());
}

#[test]
fn superseding_generation_rejects_old_results_and_old_observations() {
    let h = Harness::new(probe(&["fixture/a"]));
    h.observe("fixture", 900, &["a"]);
    let old = h.next();
    h.observe("fixture", 1000, &["a"]);
    assert!(old.1.is_cancelled());
    h.next();
    h.observe("fixture", 999, &["a"]);
    h.reply(Ok(clean()));
    let badges = h.wait("fixture", |b| b.by_name.get("a").is_some_and(|b| !b.stale));
    assert!(badges.revision >= 1000);
    assert!(h.scan.stop());
}

#[test]
fn cached_badges_publish_stale_before_fresh_and_revisions_keep_increasing() {
    let h = Harness::new(probe(&["fixture/a"]));
    h.observe("fixture", 200, &["a"]);
    h.next();
    h.reply(Ok(clean()));
    let first = h.wait("fixture", |b| b.by_name.get("a").is_some_and(|b| !b.stale));
    h.scan.rescan(&location("fixture"));
    h.next();
    let stale = h.wait("fixture", |b| b.by_name.get("a").is_some_and(|b| b.stale));
    assert!(stale.revision > first.revision);
    h.reply(Ok(status_with(b"? changed\0")));
    let fresh = h.wait("fixture", |b| b.by_name.get("a").is_some_and(|b| !b.stale));
    assert!(fresh.revision > stale.revision);
    assert_eq!(fresh.by_name["a"].kind, BadgeKind::Modified);
    assert!(h.scan.stop());
}

#[test]
fn status_timeout_publishes_unknown_without_retry_until_rescan() {
    let h = Harness::configured(
        probe(&["fixture/a"]),
        GitSettings::default(),
        Limits {
            status_timeout: Duration::from_millis(150),
            ..Limits::default()
        },
    );
    h.observe("fixture", 5, &["a"]);
    let (_, cancel) = h.next();
    h.wait("fixture", |b| {
        b.by_name
            .get("a")
            .is_some_and(|b| b.reason == Some(BadgeReason::TimedOut))
    });
    assert!(cancel.is_cancelled());
    h.observe("fixture", 5, &["a"]);
    assert!(h.calls.try_recv().is_err());
    h.scan.rescan(&location("fixture"));
    h.next();
    h.reply(Ok(clean()));
    h.wait("fixture", |b| {
        b.by_name
            .get("a")
            .is_some_and(|b| b.kind == BadgeKind::Clean && !b.stale)
    });
    assert!(h.scan.stop());
}

#[test]
fn shutdown_cancels_active_and_queued_work_and_is_idempotent() {
    let h = Harness::new(probe(&["fixture/a", "fixture/b", "fixture/c"]));
    h.observe("fixture", 1, &["a", "b", "c"]);
    let first = h.next();
    let second = h.next();
    let began = Instant::now();
    assert!(h.scan.stop());
    assert!(began.elapsed() < Duration::from_secs(2));
    assert!(first.1.is_cancelled() && second.1.is_cancelled());
    assert!(h.scan.stop());
    assert!(h.calls.try_iter().next().is_none());
}

impl Harness {
    fn wait_parked(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.scan.shared.0.lock().unwrap().parked {
            assert!(Instant::now() < deadline, "the idle scanner never slept");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn an_idle_scanner_sleeps_until_a_request_or_a_stop_wakes_it() {
    let h = Harness::new(probe(&["fixture/a", "other/a"]));
    h.observe("fixture", 1, &["a"]);
    h.next();
    h.reply(Ok(clean()));
    h.wait("fixture", |b| b.by_name.values().any(|b| !b.stale));
    h.wait_parked();
    h.observe("other", 1, &["a"]);
    h.next();
    h.reply(Ok(clean()));
    h.wait("other", |b| b.by_name.values().any(|b| !b.stale));
    h.wait_parked();
    assert!(h.scan.stop());
}

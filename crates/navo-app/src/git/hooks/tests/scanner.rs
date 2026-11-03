use super::*;

#[test]
fn badges_off_starts_nothing_and_publishes_nothing() {
    let rig = rig(&settings(false));
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert!(rig.starts.lock().unwrap().is_empty());
    assert!(!rig.hooks.running());
    assert_eq!(rig.calls(), 0);
    assert!(rig.hooks.badges(&rig.folder).is_none());
}

#[test]
fn a_fake_status_reaches_badges() {
    let rig = rig(&settings(true));
    *rig.source.reply.lock().unwrap() = Some(Ok(dirty()));
    rig.observe();
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    assert_eq!(rig.fresh_kind(), Some(BadgeKind::Modified));
}

#[test]
fn source_errors_reach_badges_as_unknown_with_their_reason() {
    for (error, reason) in [
        (ScanError::NoGit, BadgeReason::NoGit),
        (ScanError::NotTrusted, BadgeReason::NotTrusted),
    ] {
        let rig = rig(&settings(true));
        *rig.source.reply.lock().unwrap() = Some(Err(error));
        rig.observe();
        rig.wait_for(|rig| rig.fresh_kind().is_some());
        let badges = rig.hooks.badges(&rig.folder).unwrap();
        assert_eq!(badges.by_name["repo"].kind, BadgeKind::Unknown);
        assert_eq!(badges.by_name["repo"].reason, Some(reason));
    }
}

#[test]
fn toggling_off_stops_the_scanner_and_on_starts_a_new_one() {
    let rig = rig(&settings(true));
    assert!(rig.hooks.running());
    rig.hooks.apply(&settings(false));
    assert!(!rig.hooks.running());
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(rig.calls(), 0);
    rig.hooks.apply(&settings(true));
    assert!(rig.hooks.running());
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
}

#[test]
fn observations_made_while_off_are_scanned_once_it_turns_on() {
    let rig = rig(&settings(false));
    rig.observe();
    rig.hooks.apply(&settings(true));
    rig.wait_for(|rig| rig.fresh_kind().is_some());
}

#[test]
fn toggling_leaves_one_subscription_that_fires_once_per_publish() {
    let rig = rig(&settings(true));
    let fired = Arc::new(AtomicUsize::new(0));
    let counter = fired.clone();
    rig.hooks.subscribe(Box::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    for _ in 0..3 {
        rig.hooks.apply(&settings(false));
        rig.hooks.apply(&settings(true));
    }
    rig.hooks.apply(&settings(false));
    let before = fired.load(Ordering::SeqCst);
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fired.load(Ordering::SeqCst), before);
    rig.hooks.apply(&settings(true));
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    let published = fired.load(Ordering::SeqCst) - before;
    assert!((1..=3).contains(&published), "{published}");
}

#[test]
fn only_scanner_relevant_settings_restart_it() {
    let rig = rig(&settings(true));
    let mut other = settings(true);
    other.tool = "fork".to_owned();
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 1);
    other.fetch = true;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
    other.fetch_minutes = 30;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
    other.engine = GitEngine::Git;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().last(), Some(&GitEngine::Git));
    other.network = true;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 4);
}

#[test]
fn rescan_runs_the_status_again() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_for(|rig| rig.calls() == 1 && rig.fresh_kind().is_some());
    rig.hooks.rescan(&rig.folder);
    rig.wait_for(|rig| rig.calls() == 2);
}

#[test]
fn focus_rescans_only_when_the_status_is_over_60_seconds_old() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    rig.advance(Duration::from_secs(30));
    rig.hooks.focus_regained();
    rig.stays_at(1);
    rig.advance(Duration::from_secs(31));
    rig.hooks.focus_regained();
    rig.wait_for(|rig| rig.calls() == 2);
    rig.hooks.focus_regained();
    rig.stays_at(2);
}

#[test]
fn the_window_reaches_the_focus_rescan_through_the_ui_trait() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    let ui: &dyn GitHooks = &rig.hooks;
    ui.focus_regained();
    rig.stays_at(1);
    rig.advance(Duration::from_secs(61));
    ui.focus_regained();
    rig.wait_for(|rig| rig.calls() == 2);
}

#[test]
fn forgotten_folders_are_not_rescanned_on_focus() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    rig.hooks.forget(&rig.folder);
    rig.advance(Duration::from_secs(120));
    rig.hooks.focus_regained();
    rig.stays_at(1);
}

#[test]
fn the_fetch_timer_runs_only_with_badges_and_fetch_on() {
    let fetching = |badges, fetch| GitSettings {
        badges,
        fetch,
        ..GitSettings::default()
    };
    let rig = rig(&fetching(true, false));
    assert!(!rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(true, true)));
    assert!(rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(false, true)));
    assert!(!rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(true, true)));
    drop(rig.hooks.start(&fetching(true, false)));
    assert!(!rig.hooks.fetching());
}

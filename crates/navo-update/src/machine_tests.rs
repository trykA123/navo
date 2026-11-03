use super::*;
use crate::{FeedError, Found, InstallKind, Release, parse_version};
use std::time::{Duration, Instant};

fn v(text: &str) -> Version {
    parse_version(text).expect("valid version")
}

fn current() -> Version {
    v("1.0.0")
}

fn release(text: &str) -> Release {
    Release {
        version: v(text),
        size: 10,
    }
}

fn run(state: &UpdateState, event: Event, now: Instant) -> (UpdateState, Vec<Effect>) {
    step(state, event, now, &current())
}

fn started(auto: bool, staged: Option<&str>) -> Event {
    Event::Started {
        auto,
        staged: staged.map(v),
    }
}

fn installed() -> UpdateState {
    UpdateState::new(InstallKind::Installed)
}

fn checking() -> UpdateState {
    run(&installed(), started(true, None), Instant::now()).0
}

fn downloading() -> UpdateState {
    let found = Event::Checked(Ok(Found::Release(release("1.1.0"))));
    run(&checking(), found, Instant::now()).0
}

fn all_events() -> Vec<Event> {
    vec![
        started(true, None),
        started(true, Some("2.0.0")),
        Event::CheckNow,
        Event::Checked(Ok(Found::Release(release("2.0.0")))),
        Event::Checked(Err(FeedError::Offline)),
        Event::Progress(50),
        Event::Downloaded(Ok(())),
        Event::Downloaded(Err(FeedError::Interrupted)),
        Event::AutoChanged(true),
        Event::AutoChanged(false),
    ]
}

#[test]
fn machine_unsupported_never_changes_and_never_emits() {
    for kind in [InstallKind::Portable, InstallKind::Unpackaged] {
        let state = UpdateState::new(kind);
        assert_eq!(state, UpdateState::Unsupported(kind));
        for event in all_events() {
            let (next, effects) = run(&state, event, Instant::now());
            assert_eq!(next, state);
            assert!(effects.is_empty());
        }
    }
}

#[test]
fn machine_portable_and_unpackaged_start_nothing() {
    for kind in [InstallKind::Portable, InstallKind::Unpackaged] {
        let (next, effects) = run(&UpdateState::new(kind), Event::CheckNow, Instant::now());
        assert_eq!(next, UpdateState::Unsupported(kind));
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_start_with_auto_checks() {
    let (next, effects) = run(&installed(), started(true, None), Instant::now());
    assert_eq!(next, UpdateState::Checking);
    assert_eq!(effects, vec![Effect::Check]);
}

#[test]
fn machine_staged_newer_starts_ready_without_request() {
    for auto in [true, false] {
        let (next, effects) = run(&installed(), started(auto, Some("1.2.0")), Instant::now());
        assert_eq!(
            next,
            UpdateState::Ready {
                version: v("1.2.0")
            }
        );
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_staged_not_newer_is_ignored() {
    for staged in ["1.0.0", "0.9.0"] {
        let (next, effects) = run(&installed(), started(true, Some(staged)), Instant::now());
        assert_eq!(next, UpdateState::Checking);
        assert_eq!(effects, vec![Effect::Check]);
    }
}

#[test]
fn machine_auto_off_emits_nothing_at_start() {
    let (next, effects) = run(&installed(), started(false, None), Instant::now());
    assert_eq!(next, UpdateState::Off);
    assert!(effects.is_empty());
}

#[test]
fn machine_auto_off_check_now_still_checks() {
    let off = run(&installed(), started(false, None), Instant::now()).0;
    let (next, effects) = run(&off, Event::CheckNow, Instant::now());
    assert_eq!(next, UpdateState::Checking);
    assert_eq!(effects, vec![Effect::Check]);
}

#[test]
fn machine_check_now_while_busy_does_nothing() {
    for state in [checking(), downloading()] {
        let (next, effects) = run(&state, Event::CheckNow, Instant::now());
        assert_eq!(next, state);
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_found_newer_downloads() {
    let found = Event::Checked(Ok(Found::Release(release("1.1.0"))));
    let (next, effects) = run(&checking(), found, Instant::now());
    assert_eq!(
        next,
        UpdateState::Downloading {
            version: v("1.1.0"),
            percent: 0
        }
    );
    assert_eq!(effects, vec![Effect::Download(release("1.1.0"))]);
}

#[test]
fn machine_equal_or_older_is_up_to_date() {
    for text in ["1.0.0", "0.9.0"] {
        let found = Event::Checked(Ok(Found::Release(release(text))));
        let (next, effects) = run(&checking(), found, Instant::now());
        assert_eq!(next, UpdateState::UpToDate);
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_nothing_found_is_up_to_date() {
    let (next, effects) = run(
        &checking(),
        Event::Checked(Ok(Found::Nothing)),
        Instant::now(),
    );
    assert_eq!(next, UpdateState::UpToDate);
    assert!(effects.is_empty());
}

#[test]
fn machine_progress_updates_percent() {
    let (next, _) = run(&downloading(), Event::Progress(40), Instant::now());
    assert_eq!(
        next,
        UpdateState::Downloading {
            version: v("1.1.0"),
            percent: 40
        }
    );
    let (capped, _) = run(&downloading(), Event::Progress(250), Instant::now());
    assert!(matches!(
        capped,
        UpdateState::Downloading { percent: 100, .. }
    ));
}

#[test]
fn machine_download_done_is_ready() {
    let (next, effects) = run(&downloading(), Event::Downloaded(Ok(())), Instant::now());
    assert_eq!(
        next,
        UpdateState::Ready {
            version: v("1.1.0")
        }
    );
    assert!(effects.is_empty());
}

#[test]
fn machine_download_error_is_failed_never_ready() {
    for error in [FeedError::Interrupted, FeedError::Corrupt, FeedError::Busy] {
        let event = Event::Downloaded(Err(error.clone()));
        let (next, effects) = run(&downloading(), event, Instant::now());
        assert_eq!(
            next,
            UpdateState::Failed {
                error,
                retry_after: None
            }
        );
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_interrupted_download_can_be_checked_again() {
    let event = Event::Downloaded(Err(FeedError::Interrupted));
    let failed = run(&downloading(), event, Instant::now()).0;
    let (next, effects) = run(&failed, Event::CheckNow, Instant::now());
    assert_eq!(next, UpdateState::Checking);
    assert_eq!(effects, vec![Effect::Check]);
}

#[test]
fn machine_offline_at_start_fails_and_allows_retry() {
    let event = Event::Checked(Err(FeedError::Offline));
    let failed = run(&checking(), event, Instant::now()).0;
    assert_eq!(
        failed,
        UpdateState::Failed {
            error: FeedError::Offline,
            retry_after: None
        }
    );
    let (next, effects) = run(&failed, Event::CheckNow, Instant::now());
    assert_eq!(next, UpdateState::Checking);
    assert_eq!(effects, vec![Effect::Check]);
}

#[test]
fn machine_bad_feed_fails_without_wait() {
    let event = Event::Checked(Err(FeedError::BadFeed));
    let (next, effects) = run(&checking(), event, Instant::now());
    assert_eq!(
        next,
        UpdateState::Failed {
            error: FeedError::BadFeed,
            retry_after: None
        }
    );
    assert!(effects.is_empty());
}

#[test]
fn machine_rate_limited_waits_one_hour() {
    let now = Instant::now();
    let event = Event::Checked(Err(FeedError::RateLimited));
    let limited = run(&checking(), event, now).0;
    assert_eq!(
        limited,
        UpdateState::Failed {
            error: FeedError::RateLimited,
            retry_after: Some(now + Duration::from_secs(3600))
        }
    );
    let early = now + Duration::from_secs(3599);
    let (same, effects) = run(&limited, Event::CheckNow, early);
    assert_eq!(same, limited);
    assert!(effects.is_empty());
    let due = now + Duration::from_secs(3600);
    let (next, effects) = run(&limited, Event::CheckNow, due);
    assert_eq!(next, UpdateState::Checking);
    assert_eq!(effects, vec![Effect::Check]);
}

#[test]
fn machine_rate_limited_download_also_waits() {
    let now = Instant::now();
    let event = Event::Downloaded(Err(FeedError::RateLimited));
    let (next, _) = run(&downloading(), event, now);
    assert!(matches!(
        next,
        UpdateState::Failed { retry_after: Some(t), .. } if t == now + Duration::from_secs(3600)
    ));
}

#[test]
fn machine_late_answers_are_ignored() {
    let idle = installed();
    let late = [
        Event::Checked(Ok(Found::Release(release("2.0.0")))),
        Event::Downloaded(Ok(())),
        Event::Progress(10),
    ];
    for event in late {
        let (next, effects) = run(&idle, event, Instant::now());
        assert_eq!(next, idle);
        assert!(effects.is_empty());
    }
}

#[test]
fn machine_ready_ignores_check_now() {
    let ready = run(&installed(), started(true, Some("1.2.0")), Instant::now()).0;
    let (next, effects) = run(&ready, Event::CheckNow, Instant::now());
    assert_eq!(next, ready);
    assert!(effects.is_empty());
}

#[test]
fn machine_auto_changed_toggles_idle_and_off() {
    let (off, effects) = run(&installed(), Event::AutoChanged(false), Instant::now());
    assert_eq!(off, UpdateState::Off);
    assert!(effects.is_empty());
    let (idle, effects) = run(&off, Event::AutoChanged(true), Instant::now());
    assert_eq!(idle, UpdateState::Idle);
    assert!(effects.is_empty());
    let busy = checking();
    assert_eq!(
        run(&busy, Event::AutoChanged(false), Instant::now()).0,
        busy
    );
}

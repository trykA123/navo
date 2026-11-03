use super::*;
use chrono::TimeZone;
use navo_update::{InstallKind, Version};
use std::time::Duration as Std;

fn version(text: &str) -> Version {
    Version::parse(text).expect("version")
}

fn at_noon() -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 11, 3, 12, 0, 0)
        .single()
        .expect("noon")
}

fn view(state: &UpdateState) -> UpdateView {
    view_of(state, Instant::now(), at_noon())
}

fn failed(error: FeedError) -> UpdateView {
    view(&UpdateState::Failed {
        error,
        retry_after: None,
    })
}

#[test]
fn only_an_unsupported_copy_is_hidden() {
    for kind in [InstallKind::Portable, InstallKind::Unpackaged] {
        assert_eq!(view(&UpdateState::Unsupported(kind)), UpdateView::Hidden);
    }
    assert_eq!(view(&UpdateState::Idle), UpdateView::Idle);
    assert_eq!(view(&UpdateState::Off), UpdateView::Off);
    assert_eq!(view(&UpdateState::Checking), UpdateView::Checking);
    assert_eq!(view(&UpdateState::UpToDate), UpdateView::UpToDate);
}

#[test]
fn download_and_ready_carry_the_version() {
    let downloading = UpdateState::Downloading {
        version: version("0.0.1-alpha.20261103c"),
        percent: 40,
    };
    assert_eq!(
        view(&downloading),
        UpdateView::Downloading {
            version: "0.0.1-alpha.20261103c".into(),
            percent: 40
        }
    );
    let ready = UpdateState::Ready {
        version: version("0.4.0"),
    };
    assert_eq!(
        view(&ready),
        UpdateView::Ready {
            version: "0.4.0".into()
        }
    );
}

#[test]
fn every_feed_error_has_its_text() {
    let table = [
        (FeedError::Offline, UpdateFailure::Offline),
        (FeedError::BadFeed, UpdateFailure::BadFeed),
        (FeedError::Interrupted, UpdateFailure::Interrupted),
        (FeedError::Corrupt, UpdateFailure::Damaged),
        (FeedError::Busy, UpdateFailure::Busy),
        (FeedError::Other("x".into()), UpdateFailure::BadFeed),
    ];
    for (error, expected) in table {
        assert_eq!(
            failed(error.clone()),
            UpdateView::Failed(expected),
            "{error}"
        );
    }
}

#[test]
fn a_rate_limit_names_the_local_time_of_the_next_try() {
    let now = Instant::now();
    let state = UpdateState::Failed {
        error: FeedError::RateLimited,
        retry_after: Some(now + Std::from_secs(90 * 60)),
    };
    assert_eq!(
        view_of(&state, now, at_noon()),
        UpdateView::Failed(UpdateFailure::RateLimited {
            next_try: "13:30".into()
        })
    );
}

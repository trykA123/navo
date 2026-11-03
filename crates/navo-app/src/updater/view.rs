//! Maps the update machine's state to the line Settings → Help shows (D22g, D22h).

use std::time::Instant;

use chrono::{DateTime, Duration, Local};
use navo_ui::window_view::{UpdateFailure, UpdateView};
use navo_update::{FeedError, UpdateState};

/// `now` and `wall` are the same moment on the monotonic and the wall clock.
pub(crate) fn view_of(state: &UpdateState, now: Instant, wall: DateTime<Local>) -> UpdateView {
    match state {
        UpdateState::Unsupported(_) => UpdateView::Hidden,
        UpdateState::Off => UpdateView::Off,
        UpdateState::Idle => UpdateView::Idle,
        UpdateState::Checking => UpdateView::Checking,
        UpdateState::UpToDate => UpdateView::UpToDate,
        UpdateState::Downloading { version, percent } => UpdateView::Downloading {
            version: version.to_string().into(),
            percent: *percent,
        },
        UpdateState::Ready { version } => UpdateView::Ready {
            version: version.to_string().into(),
        },
        UpdateState::Failed { error, retry_after } => {
            UpdateView::Failed(failure_of(error, retry_after.map(|due| (due, now)), wall))
        }
    }
}

fn failure_of(
    error: &FeedError,
    retry: Option<(Instant, Instant)>,
    wall: DateTime<Local>,
) -> UpdateFailure {
    match error {
        FeedError::Offline => UpdateFailure::Offline,
        FeedError::RateLimited => UpdateFailure::RateLimited {
            next_try: next_try(retry, wall).into(),
        },
        FeedError::Interrupted => UpdateFailure::Interrupted,
        FeedError::Corrupt => UpdateFailure::Damaged,
        FeedError::Busy => UpdateFailure::Busy,
        FeedError::BadFeed | FeedError::Other(_) => UpdateFailure::BadFeed,
    }
}

/// The local time of day when a rate-limited feed may be asked again.
fn next_try(retry: Option<(Instant, Instant)>, wall: DateTime<Local>) -> String {
    let wait = retry.map_or(std::time::Duration::ZERO, |(due, now)| {
        due.saturating_duration_since(now)
    });
    let wait = Duration::from_std(wait).unwrap_or_default();
    (wall + wait).format("%H:%M").to_string()
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;

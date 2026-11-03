//! Update state machine: pure, time and the current version come in as arguments.

use crate::{FeedError, Found, InstallKind, Release, Version};
use std::time::{Duration, Instant};

/// How long a rate-limited feed is left alone.
const RATE_LIMIT_WAIT: Duration = Duration::from_secs(60 * 60);

/// Where an update stands. `Unsupported` never changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Unsupported(InstallKind),
    /// Installed, start-up check switched off.
    Off,
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: Version,
        percent: u8,
    },
    Ready {
        version: Version,
    },
    /// `retry_after` is set only for a rate-limited feed.
    Failed {
        error: FeedError,
        retry_after: Option<Instant>,
    },
}

impl UpdateState {
    /// The state before `Started`: only an installed copy updates.
    pub fn new(kind: InstallKind) -> Self {
        match kind {
            InstallKind::Installed => Self::Idle,
            other => Self::Unsupported(other),
        }
    }
}

/// What happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The start-up moment; `staged` is a package already downloaded.
    Started {
        auto: bool,
        staged: Option<Version>,
    },
    CheckNow,
    Checked(Result<Found, FeedError>),
    Progress(u8),
    Downloaded(Result<(), FeedError>),
    AutoChanged(bool),
}

/// What the runner must do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Check,
    Download(Release),
}

type Step = (UpdateState, Vec<Effect>);

/// Applies one event. `current` is the running version; `now` the clock.
pub fn step(state: &UpdateState, event: Event, now: Instant, current: &Version) -> Step {
    if matches!(state, UpdateState::Unsupported(_)) {
        return stay(state);
    }
    match event {
        Event::Started { auto, staged } => started(state, auto, staged, current),
        Event::CheckNow => check_now(state, now),
        Event::Checked(result) => checked(state, result, now, current),
        Event::Progress(percent) => progress(state, percent),
        Event::Downloaded(result) => downloaded(state, result, now),
        Event::AutoChanged(auto) => auto_changed(state, auto),
    }
}

fn stay(state: &UpdateState) -> Step {
    (state.clone(), Vec::new())
}

fn started(state: &UpdateState, auto: bool, staged: Option<Version>, current: &Version) -> Step {
    if !matches!(state, UpdateState::Idle | UpdateState::Off) {
        return stay(state);
    }
    if let Some(version) = staged.filter(|staged| staged > current) {
        return (UpdateState::Ready { version }, Vec::new());
    }
    if auto {
        (UpdateState::Checking, vec![Effect::Check])
    } else {
        (UpdateState::Off, Vec::new())
    }
}

fn check_now(state: &UpdateState, now: Instant) -> Step {
    let allowed = match state {
        UpdateState::Idle | UpdateState::Off | UpdateState::UpToDate => true,
        UpdateState::Failed { retry_after, .. } => retry_after.is_none_or(|due| now >= due),
        _ => false,
    };
    if allowed {
        (UpdateState::Checking, vec![Effect::Check])
    } else {
        stay(state)
    }
}

fn checked(
    state: &UpdateState,
    result: Result<Found, FeedError>,
    now: Instant,
    current: &Version,
) -> Step {
    if *state != UpdateState::Checking {
        return stay(state);
    }
    match result {
        Ok(Found::Release(release)) if release.version > *current => {
            let next = UpdateState::Downloading {
                version: release.version.clone(),
                percent: 0,
            };
            (next, vec![Effect::Download(release)])
        }
        Ok(_) => (UpdateState::UpToDate, Vec::new()),
        Err(error) => (failed(error, now), Vec::new()),
    }
}

fn progress(state: &UpdateState, percent: u8) -> Step {
    match state {
        UpdateState::Downloading { version, .. } => {
            let next = UpdateState::Downloading {
                version: version.clone(),
                percent: percent.min(100),
            };
            (next, Vec::new())
        }
        _ => stay(state),
    }
}

fn downloaded(state: &UpdateState, result: Result<(), FeedError>, now: Instant) -> Step {
    let UpdateState::Downloading { version, .. } = state else {
        return stay(state);
    };
    match result {
        Ok(()) => (
            UpdateState::Ready {
                version: version.clone(),
            },
            Vec::new(),
        ),
        Err(error) => (failed(error, now), Vec::new()),
    }
}

fn auto_changed(state: &UpdateState, auto: bool) -> Step {
    match (state, auto) {
        (UpdateState::Idle, false) => (UpdateState::Off, Vec::new()),
        (UpdateState::Off, true) => (UpdateState::Idle, Vec::new()),
        _ => stay(state),
    }
}

fn failed(error: FeedError, now: Instant) -> UpdateState {
    let retry_after = match error {
        FeedError::RateLimited => Some(now.checked_add(RATE_LIMIT_WAIT).unwrap_or(now)),
        _ => None,
    };
    UpdateState::Failed { error, retry_after }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

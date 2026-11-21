//! Decides when a check is due and runs one check against a fetch the caller passes in.

use crate::feed::{FeedError, parse_feed};
use crate::version::{Version, newest_above};

/// How long a check is trusted before another is due, in seconds.
pub const INTERVAL_S: i64 = 86_400;

/// How far a saved check time may run ahead of the clock, in seconds.
pub const CLOCK_SKEW_S: i64 = 3_600;

/// Why a check did not produce a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckError {
    /// The platform has no HTTP client for the check.
    Unsupported,
    /// WinHTTP failed with this error code.
    Network(u32),
    /// The feed answered with this status code.
    Status(u16),
    /// The body is over the feed size cap.
    TooLarge,
    /// The body is not a JSON array.
    BadFeed,
    /// The running version is not semver.
    BadRunningVersion,
}

impl CheckError {
    /// The short form stored as `last_result` in the state file.
    pub fn short(&self) -> String {
        match self {
            CheckError::Unsupported => "unsupported".into(),
            CheckError::Network(code) => format!("network {code}"),
            CheckError::Status(status) => format!("http {status}"),
            CheckError::TooLarge => "too large".into(),
            CheckError::BadFeed => "bad feed".into(),
            CheckError::BadRunningVersion => "bad running version".into(),
        }
    }
}

/// The result of one update check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A release newer than the running build.
    Newer(Version),
    /// No release is newer than the running build.
    Current,
    /// The check did not finish.
    Failed(CheckError),
}

impl Outcome {
    /// The short form stored as `last_result` in the state file.
    pub fn short(&self) -> String {
        match self {
            Outcome::Newer(version) => format!("newer {version}"),
            Outcome::Current => "current".into(),
            Outcome::Failed(error) => error.short(),
        }
    }
}

/// True when no check ran yet, the last one is a day old, or the saved time is too far ahead.
pub fn due(last: Option<i64>, now: i64) -> bool {
    let Some(last) = last else {
        return true;
    };
    last > now + CLOCK_SKEW_S || now - last >= INTERVAL_S
}

/// Runs one check with the caller's fetch; a failure is logged once.
pub fn run_check(running: &str, fetch: impl FnOnce() -> Result<Vec<u8>, CheckError>) -> Outcome {
    let outcome = evaluate(running, fetch);
    if matches!(&outcome, Outcome::Failed(_)) {
        tracing::warn!(result = %outcome.short(), "update check failed");
    }
    outcome
}

/// Turns the running version and one fetch into an outcome, without logging.
fn evaluate(running: &str, fetch: impl FnOnce() -> Result<Vec<u8>, CheckError>) -> Outcome {
    let Ok(running) = Version::parse(running) else {
        return Outcome::Failed(CheckError::BadRunningVersion);
    };
    let body = match fetch() {
        Ok(body) => body,
        Err(error) => return Outcome::Failed(error),
    };
    match parse_feed(&body) {
        Err(FeedError::TooLarge) => Outcome::Failed(CheckError::TooLarge),
        Err(FeedError::NotJson) => Outcome::Failed(CheckError::BadFeed),
        Ok(releases) => match newest_above(&running, &releases) {
            Some(version) => Outcome::Newer(version),
            None => Outcome::Current,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::MAX_FEED_BYTES;
    use std::cell::Cell;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn feed() -> &'static [u8] {
        br#"[{"tag_name":"v0.1.0-alpha.2","draft":false,"prerelease":true}]"#
    }

    #[test]
    fn no_check_or_a_day_old_one_is_due() {
        assert!(due(None, 0));
        assert!(!due(Some(0), 0));
        assert!(!due(Some(0), 86_399));
        assert!(due(Some(0), 86_400));
    }

    #[test]
    fn a_saved_time_two_hours_ahead_is_due() {
        assert!(due(Some(1_007_200), 1_000_000));
    }

    #[test]
    fn a_bad_running_version_fails_before_the_fetch() {
        let called = Cell::new(false);
        let outcome = run_check("nope", || {
            called.set(true);
            Ok(Vec::new())
        });
        assert_eq!(outcome, Outcome::Failed(CheckError::BadRunningVersion));
        assert!(!called.get());
    }

    #[test]
    fn a_network_code_has_a_short_form() {
        assert_eq!(CheckError::Network(12029).short(), "network 12029");
    }

    #[test]
    fn a_newer_alpha_wins() {
        assert_eq!(
            run_check("0.1.0-alpha.1", || Ok(feed().to_vec())),
            Outcome::Newer(v("0.1.0-alpha.2"))
        );
    }

    #[test]
    fn the_same_version_is_current() {
        assert_eq!(
            run_check("0.1.0-alpha.2", || Ok(feed().to_vec())),
            Outcome::Current
        );
    }

    #[test]
    fn an_oversized_body_is_too_large() {
        let outcome = run_check("0.1.0-alpha.1", || Ok(vec![b' '; MAX_FEED_BYTES + 1]));
        assert_eq!(outcome, Outcome::Failed(CheckError::TooLarge));
        assert_eq!(outcome.short(), "too large");
    }
}

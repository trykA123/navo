//! Tests for `sources::Sources`: which provider names reach the side hook and which are refused.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use navo_compare::model::{CancelToken, Reason};
use navo_core::{Location, ProviderId, ProviderPath};

use super::hooks::SideFn;
use super::sources::Sources;
use crate::compare::settings::CompareSettings;

/// A location on `provider` whose path text is `text`.
fn location(provider: &str, text: &str) -> Location {
    Location::with_provider(
        ProviderId::new(provider).unwrap(),
        ProviderPath::from_text(text),
    )
    .unwrap()
}

/// A side builder that records the call and then reports `NotFound`.
fn recording_side(called: &Arc<AtomicBool>) -> SideFn {
    let flag = Arc::clone(called);
    Arc::new(move |_, _| {
        flag.store(true, Ordering::SeqCst);
        Err(Reason::NotFound)
    })
}

/// A `git-remote` location reaches the side hook and its reason comes back unchanged.
#[test]
fn a_git_remote_location_reaches_the_side_hook() {
    let called = Arc::new(AtomicBool::new(false));
    let sources = Sources::new(None, recording_side(&called));
    let at = location("git-remote", "https://h/r@main/a.txt");
    let result = sources.open(&at, &CompareSettings::default(), &CancelToken::new());
    assert_eq!(result.err(), Some(Reason::NotFound));
    assert!(called.load(Ordering::SeqCst));
}

/// Another provider is refused as unsupported without reaching the side hook.
#[test]
fn another_provider_is_unsupported() {
    let called = Arc::new(AtomicBool::new(false));
    let sources = Sources::new(None, recording_side(&called));
    let at = location("zip", "archive/x.txt");
    let result = sources.open(&at, &CompareSettings::default(), &CancelToken::new());
    assert_eq!(result.err(), Some(Reason::Unsupported));
    assert!(!called.load(Ordering::SeqCst));
}

/// A plain `git` location still reaches the side hook.
#[test]
fn a_git_location_still_reaches_the_side_hook() {
    let called = Arc::new(AtomicBool::new(false));
    let sources = Sources::new(None, recording_side(&called));
    let at = location("git", "/repo/a.txt");
    let result = sources.open(&at, &CompareSettings::default(), &CancelToken::new());
    assert_eq!(result.err(), Some(Reason::NotFound));
    assert!(called.load(Ordering::SeqCst));
}

/// A `git-checkout` location reaches the side hook and its reason comes back unchanged.
#[test]
fn a_git_checkout_location_reaches_the_side_hook() {
    let called = Arc::new(AtomicBool::new(false));
    let sources = Sources::new(None, recording_side(&called));
    let at = location("git-checkout", "/repo@main/a.txt");
    let result = sources.open(&at, &CompareSettings::default(), &CancelToken::new());
    assert_eq!(result.err(), Some(Reason::NotFound));
    assert!(called.load(Ordering::SeqCst));
}

use std::time::{Duration, Instant};

use super::build::Builder;
use super::entry::Hash;
use super::*;
use crate::fixture;
use crate::source::{RecordId, WalkConfig};

fn blocked(_: &ReadOnlySource, _: &RecordId, _: u64) -> Option<(Hash, u64)> {
    std::thread::sleep(Duration::from_secs(600));
    None
}

fn slow(_: &ReadOnlySource, _: &RecordId, _: u64) -> Option<(Hash, u64)> {
    std::thread::sleep(Duration::from_secs(1));
    Some(([7; 32], 1))
}

fn unreadable(_: &ReadOnlySource, _: &RecordId, _: u64) -> Option<(Hash, u64)> {
    None
}

fn capture(source: &ReadOnlySource, config: ManifestConfig, builder: Builder) -> Manifest {
    let mut builder = builder;
    let summary = source.walk(config.walk, |record| builder.push(source, record));
    builder.finish(source, summary)
}

fn config(max_time: Duration) -> ManifestConfig {
    ManifestConfig {
        walk: WalkConfig {
            max_time,
            ..WalkConfig::default()
        },
        ..ManifestConfig::default()
    }
}

#[test]
fn a_blocked_content_read_is_abandoned_and_the_manifest_is_incomplete() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let config = config(Duration::from_millis(200));
    let builder = Builder::new(config).with_hasher(blocked, Duration::from_millis(50));
    let started = Instant::now();
    let manifest = capture(&source, config, builder);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!manifest.is_complete());
    assert_eq!(manifest.counts().hashed_bytes, 0);
}

#[test]
fn hashing_checks_the_deadline_before_each_file() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let config = config(Duration::from_millis(300));
    let builder = Builder::new(config).with_hasher(slow, Duration::from_secs(60));
    let started = Instant::now();
    let manifest = capture(&source, config, builder);
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(!manifest.is_complete());
    assert!(manifest.counts().hashed_bytes <= 2);
}

#[test]
fn hashing_within_the_deadline_stays_complete() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let manifest = Manifest::capture(&source);
    assert!(manifest.is_complete());
    assert!(manifest.counts().hashed_bytes > 0);
}

#[test]
fn a_failed_content_read_makes_the_manifest_incomplete_and_the_verdict_not_equal() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let config = ManifestConfig::default();
    let manifest = |fx_source: &ReadOnlySource| {
        let builder = Builder::new(config).with_hasher(unreadable, Duration::from_secs(5));
        capture(fx_source, config, builder)
    };
    let (before, after) = (manifest(&source), manifest(&source));
    assert!(!before.is_complete());
    assert!(before.counts().read_failures > 0);
    assert_eq!(before.counts().hashed_bytes, 0);
    let diff = Manifest::diff(&before, &after);
    assert_eq!(diff.verdict, Verdict::Incomplete);
}

use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

use super::{Capture, TARGET, capture_with, config, redactor};

const MAX: u64 = 300;
const ORDER: [&str; 3] = ["ferrite.2.log", "ferrite.1.log", "ferrite.log"];

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

fn file_lines(dir: &Path, name: &str) -> Vec<String> {
    fs::read_to_string(dir.join(name))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn emit_numbered(dir: &Path, count: u64) -> Capture {
    capture_with(config(Some(dir), MAX), redactor(), || {
        for seq in 0..count {
            tracing::info!(target: TARGET, seq, "line");
        }
    })
}

fn seqs(dir: &Path) -> Vec<u64> {
    ORDER
        .iter()
        .flat_map(|name| file_lines(dir, name))
        .map(|l| {
            let v: Value = serde_json::from_str(&l).unwrap();
            v["fields"]["seq"].as_u64().unwrap()
        })
        .collect()
}

#[test]
fn first_line_goes_to_the_stem_log() {
    let dir = TempDir::new().unwrap();
    emit_numbered(dir.path(), 1);
    assert_eq!(file_names(dir.path()), ["ferrite.log"]);
}

#[test]
fn file_line_matches_the_ring_line() {
    let dir = TempDir::new().unwrap();
    let c = emit_numbered(dir.path(), 1);
    let ring = c.only_raw();
    assert_eq!(file_lines(dir.path(), "ferrite.log"), [ring.trim_end()]);
}

#[test]
fn rolling_keeps_exactly_three_files() {
    let dir = TempDir::new().unwrap();
    emit_numbered(dir.path(), 40);
    assert_eq!(
        file_names(dir.path()),
        ["ferrite.1.log", "ferrite.2.log", "ferrite.log"]
    );
}

#[test]
fn rolled_files_stay_within_the_limit() {
    let dir = TempDir::new().unwrap();
    emit_numbered(dir.path(), 40);
    let sizes: Vec<u64> = ORDER
        .iter()
        .map(|n| fs::metadata(dir.path().join(n)).unwrap().len())
        .collect();
    assert!(sizes.iter().all(|s| *s <= MAX), "{sizes:?}");
}

#[test]
fn rolling_drops_the_oldest_lines_and_keeps_the_order() {
    let dir = TempDir::new().unwrap();
    emit_numbered(dir.path(), 40);
    let seqs = seqs(dir.path());
    let first = *seqs.first().unwrap();
    let expected: Vec<u64> = (first..40).collect();
    assert!(first > 0 && seqs == expected, "{seqs:?}");
}

#[test]
fn oversized_line_is_written_alone_to_a_fresh_file() {
    let dir = TempDir::new().unwrap();
    let big = "x".repeat(400);
    capture_with(config(Some(dir.path()), MAX), redactor(), || {
        tracing::info!(target: TARGET, "before");
        tracing::info!(target: TARGET, "{big}");
        tracing::info!(target: TARGET, "after");
    });
    let msgs: Vec<Vec<String>> = ORDER
        .iter()
        .map(|n| {
            file_lines(dir.path(), n)
                .iter()
                .map(|l| {
                    let v: Value = serde_json::from_str(l).unwrap();
                    v["msg"].as_str().unwrap().to_owned()
                })
                .collect()
        })
        .collect();
    assert_eq!(
        msgs,
        [
            vec!["before".to_owned()],
            vec![big],
            vec!["after".to_owned()]
        ]
    );
}

#[test]
fn ring_only_config_still_fills_the_ring() {
    let c = capture_with(super::ring_only(), redactor(), || {
        tracing::info!(target: TARGET, "x");
    });
    assert_eq!(c.raw_lines().len(), 1);
}

#[test]
fn ring_only_config_writes_no_files() {
    let dir = TempDir::new().unwrap();
    let mut cfg = super::ring_only();
    cfg.file_stem = dir.path().join("ferrite").to_string_lossy().into_owned();
    capture_with(cfg, redactor(), || tracing::info!(target: TARGET, "x"));
    assert!(
        file_names(dir.path()).is_empty(),
        "{:?}",
        file_names(dir.path())
    );
}

fn layer(dir: &Path) -> super::LogLayer {
    super::LogLayer::new(
        config(Some(dir), 1024 * 1024),
        redactor(),
        std::sync::Arc::new(super::EventRing::new(super::EventRing::DEFAULT_BUDGET)),
    )
    .unwrap()
}

#[test]
fn warning_flushes_info_and_itself_before_returning() {
    use tracing_subscriber::layer::SubscriberExt;

    let dir = TempDir::new().unwrap();
    let subscriber = tracing_subscriber::Registry::default().with(layer(dir.path()));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: TARGET, "buffered info");
        tracing::warn!(target: TARGET, "urgent warning");
        let lines = file_lines(dir.path(), "ferrite.log");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("buffered info"));
        assert!(lines[1].contains("urgent warning"));
    });
}

#[test]
fn error_is_on_disk_before_returning() {
    use tracing_subscriber::layer::SubscriberExt;

    let dir = TempDir::new().unwrap();
    let subscriber = tracing_subscriber::Registry::default().with(layer(dir.path()));
    tracing::subscriber::with_default(subscriber, || {
        tracing::error!(target: TARGET, "urgent error");
        assert!(file_lines(dir.path(), "ferrite.log")[0].contains("urgent error"));
    });
}

#[test]
fn explicit_flush_writes_buffered_info() {
    use tracing_subscriber::layer::SubscriberExt;

    let dir = TempDir::new().unwrap();
    let layer = layer(dir.path());
    let flush = layer.flush_handle();
    let subscriber = tracing_subscriber::Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: TARGET, "buffered info");
        flush.flush();
        assert!(file_lines(dir.path(), "ferrite.log")[0].contains("buffered info"));
    });
}

#[test]
fn background_flush_writes_info_within_one_interval() {
    use tracing_subscriber::layer::SubscriberExt;

    let dir = TempDir::new().unwrap();
    let subscriber = tracing_subscriber::Registry::default().with(layer(dir.path()));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: TARGET, "background info");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while file_lines(dir.path(), "ferrite.log").is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "background flush timed out"
            );
            std::thread::yield_now();
        }
        assert!(file_lines(dir.path(), "ferrite.log")[0].contains("background info"));
    });
}

#[test]
fn shutdown_flushes_info_and_joins_the_worker() {
    use tracing_subscriber::layer::SubscriberExt;

    let dir = TempDir::new().unwrap();
    let layer = layer(dir.path());
    let flush = layer.flush_handle();
    let subscriber = tracing_subscriber::Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: TARGET, "shutdown info");
        flush.shutdown();
        flush.shutdown();
        assert!(file_lines(dir.path(), "ferrite.log")[0].contains("shutdown info"));
    });
}

#[test]
fn info_lines_share_a_buffer_until_flush() {
    let dir = TempDir::new().unwrap();
    let mut file = super::super::rolling::RollingFile::open(
        dir.path().to_owned(),
        "ferrite".to_owned(),
        1024 * 1024,
        3,
    )
    .unwrap();
    for _ in 0..10 {
        file.write_line("info").unwrap();
    }
    assert!(file_lines(dir.path(), "ferrite.log").is_empty());
    file.flush().unwrap();
    assert_eq!(file_lines(dir.path(), "ferrite.log").len(), 10);
}

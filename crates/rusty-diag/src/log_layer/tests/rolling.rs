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

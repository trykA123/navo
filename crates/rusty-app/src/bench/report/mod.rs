mod text;

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

pub(super) use text::markdown;

#[derive(Debug, Serialize)]
pub(super) struct Report {
    pub(super) version: String,
    pub(super) commit: String,
    pub(super) os: String,
    pub(super) cpus: usize,
    pub(super) root: String,
    pub(super) files_requested: usize,
    pub(super) seed: String,
    pub(super) fixture_kept: bool,
    pub(super) generation: Generation,
    pub(super) index: Index,
    pub(super) search_quiet_wait_ms: f64,
    pub(super) search: Vec<Search>,
    pub(super) listing: Vec<Listing>,
    pub(super) incremental: Incremental,
}

#[derive(Debug, Serialize)]
pub(super) struct Generation {
    pub(super) files: usize,
    pub(super) folders: usize,
    pub(super) ms: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub(super) struct Memory {
    pub(super) private_bytes: Option<u64>,
    pub(super) resident_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(super) struct Index {
    pub(super) path: String,
    pub(super) entries: usize,
    pub(super) ms: f64,
    pub(super) entries_per_s: f64,
    pub(super) before: Memory,
    pub(super) peak: Memory,
    pub(super) after: Memory,
    pub(super) process_peak_working_set_bytes: Option<u64>,
    pub(super) snapshot_bytes: u64,
    pub(super) snapshot_save_ms: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct Search {
    pub(super) label: String,
    pub(super) query: String,
    pub(super) results: usize,
    pub(super) cold_ms: f64,
    pub(super) warm_runs: usize,
    pub(super) lock_waits: usize,
    pub(super) p50_ms: f64,
    pub(super) p95_ms: f64,
    pub(super) p99_ms: f64,
    pub(super) max_ms: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct Listing {
    pub(super) folder: String,
    pub(super) entries: usize,
    pub(super) list_ms: f64,
    pub(super) sort_ms: Vec<(String, f64)>,
    pub(super) filter: String,
    pub(super) filter_keystroke_ms: Vec<f64>,
    pub(super) filter_matches: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct Incremental {
    pub(super) measured: bool,
    pub(super) note: String,
    pub(super) created: usize,
    pub(super) renamed: usize,
    pub(super) deleted: usize,
    pub(super) operations_ms: f64,
    pub(super) reflected_after_ms: Option<f64>,
    pub(super) errors: Vec<String>,
}

pub(super) fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(super) fn percentile(sorted: &[f64], percent: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

pub(super) fn paths(out: Option<&Path>, stamp: &str) -> io::Result<(PathBuf, PathBuf)> {
    let json = match out {
        Some(path) => path.to_path_buf(),
        None => std::env::current_exe()?
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!("navo-bench-{stamp}.json")),
    };
    let md = json.with_extension("md");
    let md = if md == json {
        json.with_extension("summary.md")
    } else {
        md
    };
    Ok((json, md))
}

pub(super) fn write(report: &Report, json: &Path, md: &Path) -> io::Result<()> {
    let text = serde_json::to_string_pretty(report).map_err(io::Error::other)?;
    std::fs::write(json, text).map_err(|error| super::fixture::context("writing", json, error))?;
    std::fs::write(md, markdown(report))
        .map_err(|error| super::fixture::context("writing", md, error))
}

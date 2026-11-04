//! Anonymous data profile of the selected folder: typed counts and buckets only.

mod collect;
mod finish;
mod histogram;
mod kinds;
mod layouts;
mod reports;
#[cfg(test)]
mod tests;

use std::time::SystemTime;

use crate::privacy::FoldedExtension;
use crate::source::{ReadOnlySource, Volume, WalkConfig, WalkSummary};

pub use histogram::Histogram;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub files: u64,
    pub folders: u64,
    pub over_100_mb: u64,
    pub over_1_gb: u64,
    pub paths_over_260: u64,
    pub symlinks: u64,
    pub junctions: u64,
    pub reparse_points: u64,
    pub hidden: u64,
    pub system: u64,
    pub read_only: u64,
    pub hard_linked: u64,
    pub hard_link_checked: u64,
    pub git_repos: u64,
    pub archives: u64,
    pub images: u64,
    pub pdf: u64,
    pub elf: u64,
    pub map: u64,
    pub a2l: u64,
    pub dbc: u64,
    pub text: u64,
    pub binary: u64,
    pub sniffed: u64,
    pub time_unknown: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FolderStats {
    pub folders: u64,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionMix {
    pub extension: FoldedExtension,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub counts: Counts,
    pub folder_files: FolderStats,
    pub max_depth: u64,
    pub size: Histogram,
    pub name_length: Histogram,
    pub path_length: Histogram,
    pub depth: Histogram,
    pub repo_size: Histogram,
    pub age: Histogram,
    pub extensions: Vec<ExtensionMix>,
    pub volume: Volume,
}

/// Walks the folder read-only and returns its profile; `now` anchors the timestamp spread.
pub fn collect(
    source: &ReadOnlySource,
    config: WalkConfig,
    now: SystemTime,
) -> (Profile, WalkSummary) {
    let mut collector = collect::Collector::new(now);
    let summary = source.walk(config, |record| collector.add(source, &record));
    (collector.finish(source.volume()), summary)
}

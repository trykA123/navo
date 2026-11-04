use std::collections::HashMap;

use super::collect::{Collector, Dir, ExtensionStat};
use super::histogram::Histogram;
use super::layouts as lay;
use super::{ExtensionMix, FolderStats, Profile};
use crate::privacy::fold_extension;
use crate::source::{RecordId, Volume};

type Key = Option<RecordId>;

impl Collector {
    pub(super) fn finish(self, volume: Volume) -> Profile {
        let folder_files = folder_stats(&self.dirs);
        let repo_size = repo_sizes(&self.dirs);
        let extensions = fold_all(self.extensions, self.overflow);
        Profile {
            counts: self.counts,
            folder_files,
            max_depth: self.max_depth,
            size: self.size,
            name_length: self.name_length,
            path_length: self.path_length,
            depth: self.depth,
            repo_size,
            age: self.age,
            extensions,
            volume,
        }
    }
}

fn folder_stats(dirs: &HashMap<Key, Dir>) -> FolderStats {
    let mut counts: Vec<u64> = dirs.values().map(|dir| dir.files).collect();
    counts.sort_unstable();
    FolderStats {
        folders: u64::try_from(counts.len()).unwrap_or(u64::MAX),
        p50: percentile(&counts, 50),
        p95: percentile(&counts, 95),
        max: counts.last().copied().unwrap_or(0),
    }
}

/// Nearest-rank percentile of a sorted slice.
fn percentile(sorted: &[u64], percent: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or(0)
}

fn repo_sizes(dirs: &HashMap<Key, Dir>) -> Histogram {
    let mut totals: HashMap<Key, u64> = dirs
        .iter()
        .filter(|(_, dir)| dir.git)
        .map(|(key, _)| (key.clone(), 0))
        .collect();
    for (key, dir) in dirs {
        if let Some(repo) = nearest_repo(dirs, key)
            && let Some(total) = totals.get_mut(&repo)
        {
            *total = total.saturating_add(dir.bytes);
        }
    }
    let mut histogram = Histogram::new(&lay::REPO_SIZE);
    for bytes in totals.values() {
        histogram.add(&lay::REPO_SIZE, *bytes, *bytes);
    }
    histogram
}

/// The closest folder at or above `key` that holds a `.git` entry.
fn nearest_repo(dirs: &HashMap<Key, Dir>, key: &Key) -> Option<Key> {
    let mut current = key.clone();
    loop {
        let dir = dirs.get(&current)?;
        if dir.git {
            return Some(current);
        }
        current.as_ref()?;
        current = dir.parent.clone();
    }
}

fn fold_all(map: HashMap<String, ExtensionStat>, overflow: ExtensionStat) -> Vec<ExtensionMix> {
    let mut mix: HashMap<crate::privacy::FoldedExtension, (u64, u64)> = HashMap::new();
    let rest = std::iter::once((String::new(), overflow));
    for (text, stat) in map.into_iter().chain(rest) {
        let folders = u64::try_from(stat.folders.len()).unwrap_or(0);
        let entry = mix
            .entry(fold_extension(&text, stat.files, folders))
            .or_default();
        entry.0 += stat.files;
        entry.1 = entry.1.saturating_add(stat.bytes);
    }
    let mut rows: Vec<ExtensionMix> = mix
        .into_iter()
        .filter(|(_, (files, _))| *files > 0)
        .map(|(extension, (files, bytes))| ExtensionMix {
            extension,
            files,
            bytes,
        })
        .collect();
    rows.sort_by(|a, b| b.files.cmp(&a.files).then(b.bytes.cmp(&a.bytes)));
    rows
}

#[cfg(test)]
pub(super) fn percentile_for_tests(sorted: &[u64], percent: usize) -> u64 {
    percentile(sorted, percent)
}

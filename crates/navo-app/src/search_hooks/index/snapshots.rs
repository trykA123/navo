use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use navo_core::Location;
use navo_index::pipe::Status;
use navo_ui::command_center::index_source::drive_root;

use super::roots::contains;

const STATE: &str = "walk-coverage.json";
const HEALTHY_STARTS: u32 = 3;

/// What one start knows about the walk roots when it sweeps old snapshots.
pub(super) struct Sweep<'a> {
    pub(super) configured: &'a [Location],
    /// True when the settings name the roots; otherwise `configured` follows the drives present.
    pub(super) listed: bool,
    pub(super) in_use: &'a [Location],
    pub(super) service: Option<&'a Status>,
}

/// Removes walk snapshots of roots dropped from the settings (an absent drive keeps its own), and those of roots a healthy
/// service volume has covered for three starts in a row. Each check runs once per start.
#[derive(Default)]
pub(super) struct Snapshots {
    folder: Option<PathBuf>,
    begun: bool,
    counted: bool,
    swept: bool,
}

/// `open` stays true from the first sweep of a start until a service reply is counted; a start that
/// finds it still true had no reply, so its streaks are dropped.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct State {
    streaks: BTreeMap<String, u32>,
    open: bool,
}

pub(super) fn file_name(root: &Location) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.as_str().hash(&mut hash);
    format!("walk-{:016x}.index", hash.finish())
}

impl Snapshots {
    pub(super) fn new(folder: Option<PathBuf>) -> Self {
        Self {
            folder,
            ..Self::default()
        }
    }

    pub(super) fn sweep(&mut self, sweep: &Sweep<'_>) {
        let Some(folder) = self.folder.clone() else {
            return;
        };
        if !self.begun {
            self.begun = true;
            begin(&folder);
        }
        let in_use: BTreeSet<String> = sweep.in_use.iter().map(file_name).collect();
        if !self.swept && sweep.listed {
            self.swept = true;
            remove_unconfigured(&folder, sweep.configured, &in_use);
        }
        if let (false, Some(service)) = (self.counted, sweep.service) {
            self.counted = true;
            let covered = count_covered(&folder, sweep.configured, service);
            for name in covered.difference(&in_use) {
                remove(&folder.join(name), "covered by the index service");
            }
        }
    }
}

fn remove_unconfigured(folder: &Path, configured: &[Location], in_use: &BTreeSet<String>) {
    let wanted: BTreeSet<String> = configured.iter().map(file_name).collect();
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, "cannot list walk index snapshots");
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let snapshot = name.starts_with("walk-") && name.ends_with(".index");
        if snapshot && !wanted.contains(&name) && !in_use.contains(&name) {
            remove(&entry.path(), "its root is no longer configured");
        }
    }
}

fn load(folder: &Path) -> State {
    std::fs::read(folder.join(STATE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(folder: &Path, state: &State) -> bool {
    let saved = serde_json::to_vec(state)
        .map_err(std::io::Error::other)
        .and_then(|bytes| {
            std::fs::create_dir_all(folder)?;
            navo_store::write_atomic(&folder.join(STATE), &bytes)
        });
    if let Err(error) = &saved {
        tracing::warn!(%error, "cannot save walk index coverage");
    }
    saved.is_ok()
}

/// Marks this start open; streaks of a previous start that never counted a reply are dropped.
fn begin(folder: &Path) {
    let mut state = load(folder);
    if state.open {
        state.streaks.clear();
    }
    state.open = true;
    save(folder, &state);
}

/// Updates the per-root streak of healthy starts; returns snapshots whose streak reached three.
fn count_covered(folder: &Path, configured: &[Location], service: &Status) -> BTreeSet<String> {
    let old = load(folder).streaks;
    let streaks: BTreeMap<String, u32> = configured
        .iter()
        .filter(|root| healthy(root, service))
        .map(file_name)
        .map(|name| {
            let starts = old.get(&name).copied().unwrap_or(0).saturating_add(1);
            (name, starts)
        })
        .collect();
    let state = State {
        streaks,
        open: false,
    };
    if !save(folder, &state) {
        return BTreeSet::new();
    }
    state
        .streaks
        .into_iter()
        .filter(|(_, starts)| *starts >= HEALTHY_STARTS)
        .map(|(name, _)| name)
        .collect()
}

fn healthy(root: &Location, service: &Status) -> bool {
    service.volumes.iter().any(|volume| {
        !volume.has_blocking_error()
            && volume.entries > 0
            && Location::new(drive_root(&volume.root)).is_ok_and(|volume| contains(&volume, root))
    })
}

fn remove(path: &Path, reason: &'static str) {
    match std::fs::remove_file(path) {
        Ok(()) => tracing::info!(reason, "walk index snapshot removed"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, reason, "walk index snapshot not removed"),
    }
}

#[cfg(test)]
mod tests;

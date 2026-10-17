mod kinds;
mod largest;
mod live;
mod profile;
mod runner;
mod walk;

use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, Location};

use crate::{CancelToken, FsEntry, Provider};
use walk::{Visit, Walk};

pub use kinds::{KIND_LIMIT, Tally};
pub use largest::{LARGEST_LIMIT, LargeFile, RANKING_LIMIT, Ranking};
pub use live::{LIVE_EVERY, LiveHandle, LiveRequest, LiveSink, LiveSnapshot, spawn_live};
pub use profile::{FolderProfile, PROGRESS_EVERY, ProfileSink, ProfileState, folder_profile};
pub use runner::{ProfileHandle, spawn_profile};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderSize {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    pub skipped: u64,
}

struct SizeOnly;

impl Visit for SizeOnly {
    fn file(&mut self, _folder: &Location, _entry: &FsEntry) {}
    fn batch_end(&mut self, _totals: &FolderSize) {}
}

pub fn folder_size(
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Result<FolderSize, FsError> {
    let mut size = FolderSize::default();
    let walk = Walk {
        provider,
        root: folder,
        cancel,
    };
    walk.run(&mut size, &mut SizeOnly)?;
    Ok(size)
}

#[cfg(test)]
mod tests;

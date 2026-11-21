//! The process-wide Viewed marks store: one file-backed log in the profile, shared by every
//! compare tab so marks persist across runs and a clear reaches all tabs (VD2).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use navo_compare_profile::book::profile_text;
use navo_compare_profile::paths::open_profile;
use sidewise_host::compare::diff::viewed::{MemoryViewed, ViewedMarks};
use sidewise_host::compare::diff::viewed_file::FileViewed;

/// The profile folder choice made at start (`None`: the platform folders); unset in tests.
static PROFILE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The one store of this process, opened by the first worker that needs a mark.
static STORE: OnceLock<Arc<dyn ViewedMarks>> = OnceLock::new();

/// Remembers which profile folder the store opens in. Call once at start, before any tab; it
/// opens nothing, so the UI thread never waits on the disk.
pub(crate) fn install_viewed(explicit: Option<&Path>) {
    let _ = PROFILE.set(explicit.map(Path::to_path_buf));
}

/// The shared store. The first call opens it, so call it from a worker thread. With no
/// installed profile (tests) or when the log does not open, marks live in memory for the run.
pub(crate) fn shared() -> Arc<dyn ViewedMarks> {
    STORE
        .get_or_init(|| match PROFILE.get() {
            Some(explicit) => open_store(explicit.as_deref()),
            None => Arc::new(MemoryViewed::default()),
        })
        .clone()
}

/// Opens `data/viewed.log` of the profile (`explicit` folder, else the platform folders). A
/// failure is logged and the marks fall back to memory, so the compare still works.
pub(crate) fn open_store(explicit: Option<&Path>) -> Arc<dyn ViewedMarks> {
    let opened = open_profile(explicit)
        .map_err(|error| profile_text(&error))
        .and_then(|profile| {
            FileViewed::open(&profile.viewed_file()).map_err(|error| error.to_string())
        });
    match opened {
        Ok(file) => Arc::new(file),
        Err(error) => {
            tracing::warn!(%error, "viewed marks will not be saved this run");
            Arc::new(MemoryViewed::default())
        }
    }
}

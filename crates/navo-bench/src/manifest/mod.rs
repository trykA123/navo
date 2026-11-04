//! Before/after proof that a run changed nothing in the owner's folder.
//!
//! A manifest holds hashes and numbers only. Names and paths are hashed on
//! the way in and never stored, printed or serialised.
//!
//! Blind spots: a change it cannot see.
//! - Content of files over 1 MiB that keep their size and modified time.
//! - Alternate data streams, ACLs and creation times.
//! - The chosen folder's own modified time.
//! - Anything behind a junction or link; only the link text is hashed.

mod build;
mod diff;
mod entry;
mod hashing;

use std::fmt;

use serde::{Serialize, Serializer};

pub use diff::{ManifestDiff, Verdict};

use crate::source::{ReadOnlySource, WalkConfig};
use build::Builder;
use entry::{Entry, Hash};

/// Largest file whose content is hashed.
pub const SMALL_FILE_BYTES: u64 = 1024 * 1024;
/// Total content bytes hashed in one capture.
pub const CONTENT_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct ManifestConfig {
    pub walk: WalkConfig,
    pub small_file_bytes: u64,
    pub content_budget: u64,
}

impl Default for ManifestConfig {
    fn default() -> Self {
        Self {
            walk: WalkConfig::default(),
            small_file_bytes: SMALL_FILE_BYTES,
            content_budget: CONTENT_BUDGET_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Counts {
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    pub others: u64,
    pub bytes: u64,
    pub hashed_bytes: u64,
    /// Small files whose content could not be read; the manifest is then incomplete.
    pub read_failures: u64,
}

/// Sorted per-entry records plus one digest over all of them.
#[derive(Clone)]
pub struct Manifest {
    entries: Vec<Entry>,
    digest: Hash,
    counts: Counts,
    complete: bool,
}

#[derive(Serialize)]
struct Summary {
    digest: String,
    complete: bool,
    #[serde(flatten)]
    counts: Counts,
}

impl Manifest {
    pub fn capture(source: &ReadOnlySource) -> Self {
        Self::capture_with(source, ManifestConfig::default())
    }

    /// An incomplete walk (cap, time or read error) marks the manifest incomplete.
    pub fn capture_with(source: &ReadOnlySource, config: ManifestConfig) -> Self {
        let mut builder = Builder::new(config);
        let summary = source.walk(config.walk, |record| builder.push(source, record));
        builder.finish(source, summary)
    }

    pub fn diff(before: &Self, after: &Self) -> ManifestDiff {
        diff::compare(before, after)
    }

    pub fn counts(&self) -> Counts {
        self.counts
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn digest_hex(&self) -> String {
        blake3::Hash::from_bytes(self.digest).to_hex().to_string()
    }

    fn summary(&self) -> Summary {
        Summary {
            digest: self.digest_hex(),
            complete: self.complete,
            counts: self.counts,
        }
    }
}

impl fmt::Debug for Manifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Manifest")
            .field("counts", &self.counts)
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for Manifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let counts = &self.counts;
        write!(
            f,
            "manifest {}: {} files, {} folders, {} links, {} bytes",
            if self.complete {
                "complete"
            } else {
                "incomplete"
            },
            counts.files,
            counts.dirs,
            counts.links,
            counts.bytes,
        )
    }
}

impl Serialize for Manifest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.summary().serialize(serializer)
    }
}

#[cfg(test)]
mod end_to_end_tests;
#[cfg(test)]
mod hashing_tests;
#[cfg(test)]
mod mutate;
#[cfg(test)]
mod tests;

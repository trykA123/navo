use std::fs::{File, Metadata};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use super::identity;

pub(super) struct Baseline {
    held: File,
    len: u64,
    modified: Option<SystemTime>,
    invalid: AtomicBool,
}

#[cfg(test)]
thread_local! {
    static FAIL_CLONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes the next `Baseline::new` on this thread see a failed handle clone.
#[cfg(test)]
pub(super) fn fail_next_clone() {
    FAIL_CLONE.with(|fail| fail.set(true));
}

fn clone_handle(file: &File) -> io::Result<File> {
    #[cfg(test)]
    if FAIL_CLONE.with(|fail| fail.replace(false)) {
        return Err(io::Error::other("descriptors exhausted"));
    }
    file.try_clone()
}

impl Baseline {
    /// Fails when the handle cannot be cloned: without it a replacement of the same size and time would pass.
    pub(super) fn new(file: &File, metadata: &Metadata) -> io::Result<Self> {
        Ok(Self {
            held: clone_handle(file)?,
            len: metadata.len(),
            modified: metadata.modified().ok(),
            invalid: AtomicBool::new(false),
        })
    }

    pub(super) fn check(&self, path: &Path, revision: &AtomicU64) -> io::Result<bool> {
        if self.invalid.load(Ordering::Acquire) {
            return Ok(false);
        }
        let named = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.invalid.store(true, Ordering::Release);
                revision.fetch_add(1, Ordering::AcqRel);
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let named_metadata = named.metadata()?;
        let same = identity::same_identity(&self.held, &named);
        let replaced = match same {
            Some(same) => !same,
            None => {
                named_metadata.len() != self.len || named_metadata.modified().ok() != self.modified
            }
        };
        let changed = replaced
            || named_metadata.len() != self.len
            || named_metadata.modified().ok() != self.modified
            || !named_metadata.is_file();
        if changed {
            self.invalid.store(true, Ordering::Release);
            revision.fetch_add(1, Ordering::AcqRel);
        }
        Ok(!changed)
    }
}

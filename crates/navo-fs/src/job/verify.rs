//! Verify-after-copy: the per-job mode, the shared counters, and the check `write()` runs
//! before a copied temporary file is placed. The job card reads the counters.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use navo_core::{FsError, FsErrorKind, Location};

use super::copy::{Copier, CopyStep, failure};
use crate::{DriveClass, ReadSource};

/// When a job compares each copied file with its source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerifyMode {
    Off,
    #[default]
    Auto,
    Always,
}

impl VerifyMode {
    /// Says whether a copy to a drive of class `target` is verified: `Auto` only for removable
    /// drives and network shares, because an unknown class may be either.
    #[must_use]
    pub fn verifies(self, target: DriveClass) -> bool {
        match self {
            Self::Off => false,
            Self::Always => true,
            Self::Auto => matches!(target, DriveClass::Removable | DriveClass::Network),
        }
    }
}

/// Counters shared by a job and its card: files verified, files left unverified, and how many
/// hash passes run now. Relaxed ordering throughout: these are counters and no data hangs on them.
#[derive(Debug, Default)]
pub struct VerifyCount {
    verified: AtomicU64,
    unverified: AtomicU64,
    verifying: AtomicU32,
}

impl VerifyCount {
    /// Number of files whose copy matched the source.
    #[must_use]
    pub fn verified(&self) -> u64 {
        self.verified.load(Ordering::Relaxed)
    }

    /// Number of files copied without a comparison (for example written by the source program).
    #[must_use]
    pub fn unverified(&self) -> u64 {
        self.unverified.load(Ordering::Relaxed)
    }

    /// True while at least one hash pass runs.
    #[must_use]
    pub fn verifying(&self) -> bool {
        self.verifying.load(Ordering::Relaxed) > 0
    }

    /// Counts one file whose copy matched the source.
    pub(crate) fn note_verified(&self) {
        self.verified.fetch_add(1, Ordering::Relaxed);
    }

    /// Counts one file copied without a comparison.
    pub(crate) fn note_unverified(&self) {
        self.unverified.fetch_add(1, Ordering::Relaxed);
    }

    /// Marks a hash pass as running until the returned guard is dropped (RAII guard).
    pub(crate) fn begin(&self) -> Verifying<'_> {
        self.verifying.fetch_add(1, Ordering::Relaxed);
        Verifying { count: self }
    }
}

/// Guard for one running hash pass; dropping it ends the pass.
pub(crate) struct Verifying<'a> {
    count: &'a VerifyCount,
}

impl Drop for Verifying<'_> {
    /// Ends the hash pass this guard stands for.
    fn drop(&mut self) {
        self.count.verifying.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Where the expected (source) hash of a copied file comes from.
pub(super) enum SourceDigest {
    /// Hash the source again with a second, cached read.
    ReadAgain,
    /// BLAKE3 of the bytes as the stream read them.
    #[expect(dead_code, reason = "AD9b hashes streamed sources")]
    Streamed([u8; 32]),
    /// The source cannot be read twice; the file counts as unverified.
    Unavailable,
}

/// Compares a copied temporary file `temp` with the source of `step` when the job verifies.
/// Hashes the source first, syncs `temp`, then hashes it with an uncached read (BLAKE3).
/// Fails with `VerifyFailed` on a mismatch; hash and flush errors (`NotFound`, `Cancelled`)
/// are returned as they are.
pub(super) fn check(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    source: SourceDigest,
) -> Result<(), FsError> {
    let Some(count) = copier.verify else {
        return Ok(());
    };
    let (ops, cancel) = (copier.ops, copier.cancel);
    let streamed = match source {
        SourceDigest::ReadAgain => None,
        SourceDigest::Streamed(digest) => Some(digest),
        SourceDigest::Unavailable => {
            count.note_unverified();
            tracing::info!(
                step = step.seq,
                "copy not verified: its source cannot be read twice"
            );
            return Ok(());
        }
    };
    let _running = count.begin();
    let expected = match streamed {
        Some(digest) => digest,
        None => ops.content_hash(step.source, cancel)?,
    };
    ops.flush(temp)?;
    let actual = ops.content_hash_uncached(temp, cancel)?;
    if actual.hash != expected {
        return Err(failure(FsErrorKind::VerifyFailed, step.target));
    }
    match actual.source {
        ReadSource::Device => count.note_verified(),
        ReadSource::CacheFallback => count.note_unverified(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_by_mode_and_target_class() {
        let classes = [
            DriveClass::Ssd,
            DriveClass::Hdd,
            DriveClass::Network,
            DriveClass::Removable,
            DriveClass::Unknown,
        ];
        let table: Vec<(VerifyMode, Vec<bool>)> =
            [VerifyMode::Off, VerifyMode::Auto, VerifyMode::Always]
                .into_iter()
                .map(|mode| {
                    (
                        mode,
                        classes.iter().map(|&class| mode.verifies(class)).collect(),
                    )
                })
                .collect();
        assert_eq!(
            table,
            [
                (VerifyMode::Off, vec![false, false, false, false, false]),
                (VerifyMode::Auto, vec![false, false, true, true, false]),
                (VerifyMode::Always, vec![true, true, true, true, true]),
            ]
        );
    }

    #[test]
    fn default_mode_is_auto() {
        assert_eq!(VerifyMode::default(), VerifyMode::Auto);
    }

    #[test]
    fn begin_marks_verifying_until_dropped() {
        let count = VerifyCount::default();
        assert!(!count.verifying());
        let guard = count.begin();
        assert!(count.verifying());
        drop(guard);
        assert!(!count.verifying());
    }

    #[test]
    fn verifying_holds_while_any_of_two_guards_lives() {
        let count = VerifyCount::default();
        let first = count.begin();
        let second = count.begin();
        drop(first);
        assert!(count.verifying());
        drop(second);
        assert!(!count.verifying());
    }

    #[test]
    fn notes_count_verified_and_unverified_files() {
        let count = VerifyCount::default();
        count.note_verified();
        count.note_verified();
        count.note_unverified();
        assert_eq!((count.verified(), count.unverified()), (2, 1));
    }
}

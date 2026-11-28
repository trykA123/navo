//! Hard-link deduplication (Stage 1.5), parallel 4 KiB head/tail xxhash (Stage 2),
//! and parallel streaming BLAKE3 verification (Stage 3).

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{ErrorKind, Read, SeekFrom};
use std::ops::ControlFlow;

use navo_core::FsErrorKind;
use rayon::prelude::*;
use xxhash_rust::xxh3::{Xxh3, xxh3_64};

use super::walk::{Candidate, SizeBucket};
use super::{
    DupesError, DupesRead, DupesSource, DuplicateEntry, DuplicateGroup, DuplicateSummary,
    FULL_HASH_CHUNK_BYTES, PARTIAL_HASH_BYTES,
};
use crate::{CancelToken, FileIdentity, FileIdentityKind};

/// Target candidate count per parallel batch so streaming stays responsive while rayon stays busy.
const BATCH_CANDIDATE_TARGET: usize = 128;

/// Physical inode key extracted from a [`FileIdentity`] for hard-link deduplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PhysicalIdentity {
    volume: u64,
    file: [u8; 16],
    kind: u8,
}

impl PhysicalIdentity {
    fn from_file_identity(identity: FileIdentity) -> Option<Self> {
        let kind = match identity.kind {
            FileIdentityKind::Native => 0,
            FileIdentityKind::WindowsIndex => 1,
            // Fingerprints use (size, mtime) and cannot prove two paths share an inode.
            FileIdentityKind::Fingerprint { .. } => return None,
        };
        Some(Self {
            volume: identity.volume,
            file: identity.file,
            kind,
        })
    }
}

#[derive(Debug, Clone)]
struct IdentifiedCandidate {
    candidate: Candidate,
    identity: Option<FileIdentity>,
}

enum HashOutcome<T> {
    Hashed(T),
    Cancelled,
    IoError,
}

enum ReadState {
    Ok,
    Cancelled,
    IoError,
}

/// Processes Stage 1 size buckets in streaming parallel batches through hard-link deduplication,
/// Stage 2 partial xxhash, and Stage 3 full BLAKE3 hashing.
pub(super) fn process_buckets(
    source: &dyn DupesSource,
    buckets: Vec<SizeBucket>,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
    sink: &mut impl FnMut(DuplicateGroup) -> ControlFlow<()>,
) -> Result<(), DupesError> {
    let mut batch = Vec::new();
    let mut batch_files = 0_usize;

    for bucket in buckets {
        if cancel.is_cancelled() {
            return Err(DupesError::Cancelled);
        }
        batch_files = batch_files.saturating_add(bucket.candidates.len());
        batch.push(bucket);
        if batch_files >= BATCH_CANDIDATE_TARGET {
            let drained = std::mem::take(&mut batch);
            batch_files = 0;
            if process_batch(source, drained, cancel, summary, sink)?.is_break() {
                return Ok(());
            }
        }
    }

    if !batch.is_empty() {
        let _ = process_batch(source, batch, cancel, summary, sink)?;
    }
    Ok(())
}

fn process_batch(
    source: &dyn DupesSource,
    buckets: Vec<SizeBucket>,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
    sink: &mut impl FnMut(DuplicateGroup) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, DupesError> {
    let deduped = deduplicate_hard_links(source, buckets, cancel, summary)?;
    if deduped.is_empty() {
        return Ok(ControlFlow::Continue(()));
    }

    let partial_collisions = stage_partial_hash(source, deduped, cancel, summary)?;
    if partial_collisions.is_empty() {
        return Ok(ControlFlow::Continue(()));
    }

    let groups = stage_full_hash(source, partial_collisions, cancel, summary)?;
    for group in groups {
        if cancel.is_cancelled() {
            return Err(DupesError::Cancelled);
        }
        record_group(summary, &group);
        if sink(group).is_break() {
            return if cancel.is_cancelled() {
                Err(DupesError::Cancelled)
            } else {
                Ok(ControlFlow::Break(()))
            };
        }
    }
    Ok(ControlFlow::Continue(()))
}

/// Deduplicates each same-size bucket by physical [`FileIdentity`] before opening any file.
fn deduplicate_hard_links(
    source: &dyn DupesSource,
    buckets: Vec<SizeBucket>,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
) -> Result<Vec<IdentifiedCandidate>, DupesError> {
    let mut surviving = Vec::new();
    for bucket in buckets {
        let mut seen = HashSet::new();
        let mut unique = Vec::with_capacity(bucket.candidates.len());
        for candidate in bucket.candidates {
            if cancel.is_cancelled() {
                return Err(DupesError::Cancelled);
            }
            let identity = match source.file_identity(&candidate.location) {
                Ok(identity) => identity,
                Err(error) if error.kind == FsErrorKind::Cancelled => {
                    return Err(DupesError::Cancelled);
                }
                Err(_) => {
                    summary.skipped_errors += 1;
                    continue;
                }
            };
            if let Some(phys) = identity.and_then(PhysicalIdentity::from_file_identity)
                && !seen.insert(phys)
            {
                summary.skipped_hard_links += 1;
                continue;
            }
            unique.push(IdentifiedCandidate {
                candidate,
                identity,
            });
        }
        if unique.len() >= 2 {
            surviving.extend(unique);
        }
    }
    Ok(surviving)
}

/// Stage 2: computes 4 KiB head + 4 KiB tail xxhash in parallel and keeps only colliding buckets.
fn stage_partial_hash(
    source: &dyn DupesSource,
    candidates: Vec<IdentifiedCandidate>,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
) -> Result<Vec<IdentifiedCandidate>, DupesError> {
    let outcomes: Vec<(IdentifiedCandidate, HashOutcome<u64>)> = candidates
        .into_par_iter()
        .map(|item| {
            let outcome = partial_hash_one(source, &item.candidate, cancel);
            (item, outcome)
        })
        .collect();

    let mut by_partial: HashMap<(u64, u64), Vec<IdentifiedCandidate>> = HashMap::new();
    for (item, outcome) in outcomes {
        match outcome {
            HashOutcome::Hashed(digest) => {
                by_partial
                    .entry((item.candidate.size, digest))
                    .or_default()
                    .push(item);
            }
            HashOutcome::Cancelled => return Err(DupesError::Cancelled),
            HashOutcome::IoError => summary.skipped_errors += 1,
        }
    }
    if cancel.is_cancelled() {
        return Err(DupesError::Cancelled);
    }

    let mut collisions = Vec::new();
    for (_, group) in by_partial {
        if group.len() >= 2 {
            collisions.extend(group);
        }
    }
    Ok(collisions)
}

/// Stage 3: computes streaming full BLAKE3 hashes in parallel and builds sorted duplicate groups.
fn stage_full_hash(
    source: &dyn DupesSource,
    candidates: Vec<IdentifiedCandidate>,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
) -> Result<Vec<DuplicateGroup>, DupesError> {
    let outcomes: Vec<(IdentifiedCandidate, HashOutcome<[u8; 32]>)> = candidates
        .into_par_iter()
        .map(|item| {
            let outcome = full_hash_one(source, &item.candidate, cancel);
            (item, outcome)
        })
        .collect();

    let mut by_full: BTreeMap<(Reverse<u64>, [u8; 32]), Vec<DuplicateEntry>> = BTreeMap::new();
    for (item, outcome) in outcomes {
        match outcome {
            HashOutcome::Hashed(hash) => {
                let entry = DuplicateEntry {
                    location: item.candidate.location,
                    size: item.candidate.size,
                    modified_ms: item.candidate.modified_ms,
                    identity: item.identity,
                };
                by_full
                    .entry((Reverse(item.candidate.size), hash))
                    .or_default()
                    .push(entry);
            }
            HashOutcome::Cancelled => return Err(DupesError::Cancelled),
            HashOutcome::IoError => summary.skipped_errors += 1,
        }
    }
    if cancel.is_cancelled() {
        return Err(DupesError::Cancelled);
    }

    let mut groups = Vec::new();
    for ((Reverse(size), hash), mut entries) in by_full {
        if entries.len() >= 2 {
            entries.sort_by(|left, right| left.location.as_str().cmp(right.location.as_str()));
            groups.push(DuplicateGroup {
                size,
                hash,
                entries,
            });
        }
    }
    Ok(groups)
}

fn partial_hash_one(
    source: &dyn DupesSource,
    candidate: &Candidate,
    cancel: &CancelToken,
) -> HashOutcome<u64> {
    if cancel.is_cancelled() {
        return HashOutcome::Cancelled;
    }
    let mut reader = match open_reader(source, &candidate.location) {
        Ok(reader) => reader,
        Err(outcome) => return outcome,
    };
    let window = u64::try_from(PARTIAL_HASH_BYTES).unwrap_or(4096);
    let double_window = window.saturating_mul(2);
    if candidate.size <= double_window {
        let len = usize::try_from(candidate.size).unwrap_or(PARTIAL_HASH_BYTES * 2);
        let mut buf = vec![0_u8; len];
        return match read_exact_checked(&mut *reader, &mut buf, cancel) {
            ReadState::Ok => HashOutcome::Hashed(xxh3_64(&buf)),
            ReadState::Cancelled => HashOutcome::Cancelled,
            ReadState::IoError => HashOutcome::IoError,
        };
    }

    let mut head = [0_u8; PARTIAL_HASH_BYTES];
    match read_exact_checked(&mut *reader, &mut head, cancel) {
        ReadState::Ok => {}
        ReadState::Cancelled => return HashOutcome::Cancelled,
        ReadState::IoError => return HashOutcome::IoError,
    }
    let Some(tail_offset) = candidate.size.checked_sub(window) else {
        return HashOutcome::IoError;
    };
    if reader.seek(SeekFrom::Start(tail_offset)).is_err() {
        return HashOutcome::IoError;
    }
    let mut tail = [0_u8; PARTIAL_HASH_BYTES];
    match read_exact_checked(&mut *reader, &mut tail, cancel) {
        ReadState::Ok => {}
        ReadState::Cancelled => return HashOutcome::Cancelled,
        ReadState::IoError => return HashOutcome::IoError,
    }
    let mut hasher = Xxh3::new();
    hasher.update(&head);
    hasher.update(&tail);
    HashOutcome::Hashed(hasher.digest())
}

fn full_hash_one(
    source: &dyn DupesSource,
    candidate: &Candidate,
    cancel: &CancelToken,
) -> HashOutcome<[u8; 32]> {
    if cancel.is_cancelled() {
        return HashOutcome::Cancelled;
    }
    let mut reader = match open_reader(source, &candidate.location) {
        Ok(reader) => reader,
        Err(outcome) => return outcome,
    };
    let mut buf = vec![0_u8; FULL_HASH_CHUNK_BYTES];
    let mut hasher = blake3::Hasher::new();
    let mut total = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return HashOutcome::Cancelled;
        }
        let read = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return HashOutcome::IoError,
        };
        if let Some(chunk) = buf.get(..read) {
            hasher.update(chunk);
        }
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > candidate.size {
            return HashOutcome::IoError;
        }
    }
    if cancel.is_cancelled() {
        return HashOutcome::Cancelled;
    }
    if total != candidate.size {
        return HashOutcome::IoError;
    }
    HashOutcome::Hashed(*hasher.finalize().as_bytes())
}

fn open_reader<T>(
    source: &dyn DupesSource,
    location: &navo_core::Location,
) -> Result<Box<dyn DupesRead>, HashOutcome<T>> {
    source.open_read(location).map_err(|error| {
        if error.kind == FsErrorKind::Cancelled {
            HashOutcome::Cancelled
        } else {
            HashOutcome::IoError
        }
    })
}

fn read_exact_checked(reader: &mut dyn Read, buf: &mut [u8], cancel: &CancelToken) -> ReadState {
    let mut filled = 0_usize;
    while filled < buf.len() {
        if cancel.is_cancelled() {
            return ReadState::Cancelled;
        }
        let Some(rest) = buf.get_mut(filled..) else {
            return ReadState::IoError;
        };
        match reader.read(rest) {
            Ok(0) => return ReadState::IoError,
            Ok(read) => filled = filled.saturating_add(read),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return ReadState::IoError,
        }
    }
    if cancel.is_cancelled() {
        ReadState::Cancelled
    } else {
        ReadState::Ok
    }
}

fn record_group(summary: &mut DuplicateSummary, group: &DuplicateGroup) {
    let count = u64::try_from(group.entries.len()).unwrap_or(u64::MAX);
    summary.duplicate_groups = summary.duplicate_groups.saturating_add(1);
    summary.duplicate_files = summary.duplicate_files.saturating_add(count);
    summary.reclaimable_bytes = summary
        .reclaimable_bytes
        .saturating_add(group.reclaimable_bytes());
}

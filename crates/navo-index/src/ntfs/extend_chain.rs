//! Recognises NTFS metadata folders below `$Extend` after a restore, when the index has no
//! metadata-folder set yet: reads an unknown parent's MFT record and walks its parent chain.

use std::collections::{HashMap, HashSet};

use super::super::skipped::is_extend;
use super::super::{NtfsError, NtfsIndex, is_root};
use super::VolumeIo;
use crate::usn::{UsnBatch, parse_first};

/// Most MFT records one walk reads; `$Deleted` and `$Txf` sit one or two levels below `$Extend`.
pub(super) const MAX_CHAIN: usize = 8;
const RECORD_MASK: u64 = 0x0000_ffff_ffff_ffff;
const FILE_CREATE: u32 = 0x100;
const RENAME_NEW_NAME: u32 = 0x2000;

/// What a walk learned of a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// It lies below `$Extend`; `reads` = records from it up to the stop, itself included.
    Below(usize),
    /// Its chain reached the root or an indexed entry: proved not metadata.
    Outside,
    /// A record was missing, unparsable or reused, or the bound was hit: no answer.
    Unanswered,
}

impl Reach {
    /// Adds `reads` records below this verdict; only `Below` counts them.
    fn plus(self, reads: usize) -> Self {
        match self {
            Self::Below(above) => Self::Below(above + reads),
            other => other,
        }
    }
}

/// The per-batch cache: the [`Reach`] of each reference a walk read.
type Verdicts = HashMap<u128, Reach>;

/// Answers a reference without reading it, as in [`Verdicts`]: `$Extend` and known metadata
/// folders are metadata after 0 reads; the root and indexed entries are outside; anything else
/// not yet cached is unknown (`None`). A skipped folder is unknown too: the full build can leave
/// a folder below `$Extend` (as `$Deleted`) in the skipped set only, so it is read.
fn known_verdict(reference: u128, index: &NtfsIndex, verdicts: &Verdicts) -> Option<Reach> {
    if is_extend(reference) || index.metadata_folders.contains(&reference) {
        return Some(Reach::Below(0));
    }
    if is_root(reference) || index.ids.contains_key(&reference) {
        return Some(Reach::Outside);
    }
    verdicts.get(&reference).copied()
}

/// Finds where `start` leads within [`MAX_CHAIN`] reads, by reading its MFT record and its
/// parents' until a reference with a known verdict; stores the verdict of every reference read.
/// A record slot holding another full reference (sequence number included) or a missing or
/// unparsable record leaves the whole path unanswered, whatever the order. A bound hit leaves
/// `start` unanswered and caches nothing, so a shorter walk through the same records still
/// succeeds. `$Extend` itself is never read: enumeration does not return record 11. Fails with
/// the read error and caches nothing then.
fn walk(
    volume: &mut impl VolumeIo,
    start: u128,
    index: &NtfsIndex,
    verdicts: &mut Verdicts,
) -> Result<Reach, NtfsError> {
    let mut path = Vec::new();
    let mut asked = start;
    let verdict = loop {
        if let Some(known) = known_verdict(asked, index, verdicts) {
            break known;
        }
        if path.len() == MAX_CHAIN {
            return Ok(Reach::Unanswered);
        }
        let Ok(reference) = u64::try_from(asked) else {
            break Reach::Unanswered;
        };
        let Some(bytes) = volume.enumerate_one(reference & RECORD_MASK)? else {
            break Reach::Unanswered;
        };
        let Ok(Some(record)) = parse_first(&bytes) else {
            break Reach::Unanswered;
        };
        path.push(asked);
        if record.file_id != asked {
            break Reach::Unanswered;
        }
        asked = record.parent_id;
    };
    let reads = path.len();
    for (depth, reference) in path.into_iter().enumerate() {
        verdicts.insert(reference, verdict.plus(reads - depth));
    }
    Ok(verdict.plus(reads))
}

/// Lists this batch's parent references, once each in first-seen order, that the index does not
/// know as metadata, indexed or root, with whether a new name or create record names a child.
fn candidate_parents(batch: &UsnBatch, index: &NtfsIndex) -> Vec<(u128, bool)> {
    // A parent this batch creates or renames in resolves through the batch itself.
    let named: HashSet<u128> = batch
        .records
        .iter()
        .filter(|record| record.reason & (FILE_CREATE | RENAME_NEW_NAME) != 0)
        .map(|record| record.file_id)
        .collect();
    let mut slots: HashMap<u128, usize> = HashMap::new();
    let mut parents: Vec<(u128, bool)> = Vec::new();
    for record in &batch.records {
        let parent = record.parent_id;
        if is_root(parent)
            || is_extend(parent)
            || named.contains(&parent)
            || index.ids.contains_key(&parent)
            || index.metadata_folders.contains(&parent)
        {
            continue;
        }
        let names = record.reason & (FILE_CREATE | RENAME_NEW_NAME) != 0;
        let slot = *slots.entry(parent).or_insert_with(|| {
            parents.push((parent, false));
            parents.len() - 1
        });
        parents[slot].1 |= names;
    }
    parents
}

/// Lists this batch's parent references whose MFT parent chain reaches `$Extend`. A skipped-only
/// folder is walked, and an unanswered walk asks the path by id, only for a new name or create
/// under it, so data records cost nothing. Every reference is read at most once per batch (the
/// `verdicts` cache); the first read error ends the search, so the remaining changes stay blocked.
pub(in crate::ntfs) fn metadata_parents(
    volume: &mut impl VolumeIo,
    batch: &UsnBatch,
    index: &NtfsIndex,
) -> Vec<u128> {
    let mut verdicts = Verdicts::new();
    let mut found = Vec::new();
    for (parent, names) in candidate_parents(batch, index) {
        if !names && index.skipped_folders.contains(&parent) {
            continue;
        }
        match walk(volume, parent, index, &mut verdicts) {
            Ok(Reach::Below(reads)) if reads <= MAX_CHAIN => found.push(parent),
            Ok(Reach::Outside) => {}
            Ok(_) if names && under_extend_by_path(volume, parent) => found.push(parent),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "metadata folder check failed; its changes stay blocked");
                break;
            }
        }
    }
    found
}

/// Asks the volume for a parent's path by its full reference when the walk did not place it.
/// True only for a path below `\$Extend\` (any letter case); no answer or an error (a gone or
/// reused reference, a denied open) keeps the parent "not metadata", so its changes stay blocked.
fn under_extend_by_path(volume: &mut impl VolumeIo, reference: u128) -> bool {
    const PREFIX: &str = "\\$extend\\";
    match volume.path_by_id(reference) {
        Ok(Some(path)) => {
            let head = String::from_utf16_lossy(path.get(..PREFIX.len()).unwrap_or_default());
            head.eq_ignore_ascii_case(PREFIX)
        }
        Ok(None) => false,
        Err(error) => {
            tracing::debug!(%error, "metadata path check failed; its changes stay blocked");
            false
        }
    }
}

use std::mem::size_of;
use std::ops::Range;

use super::diff::Hunk;

pub const HUNK_DIGEST_BYTES: usize = 16;

/// BLAKE3 digests of a copyable hunk's left and right byte ranges, truncated to 128 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkDigest {
    pub left: [u8; HUNK_DIGEST_BYTES],
    pub right: [u8; HUNK_DIGEST_BYTES],
}

impl HunkDigest {
    #[must_use]
    pub fn of(left: &[u8], right: &[u8]) -> Self {
        Self {
            left: digest(left),
            right: digest(right),
        }
    }
}

/// The stable id of a hunk: the file offset where its source range starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HunkId(u64);

impl HunkId {
    pub(crate) fn of(hunk: &Hunk) -> Self {
        Self(hunk.left_bytes.start)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HunkDigestEntry {
    pub(crate) id: HunkId,
    pub(crate) digest: HunkDigest,
}

pub(crate) const HUNK_DIGEST_ENTRY_BYTES: u64 = size_of::<HunkDigestEntry>() as u64;

/// Digests for copyable hunks, keyed by `HunkId` and kept in hunk order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HunkDigests {
    pub(crate) entries: Vec<HunkDigestEntry>,
}

impl HunkDigests {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, hunk: &Hunk, digest: HunkDigest) {
        let id = HunkId::of(hunk);
        debug_assert!(
            self.entries.last().is_none_or(|entry| entry.id < id),
            "digests are pushed in hunk order"
        );
        self.entries.push(HunkDigestEntry { id, digest });
    }

    #[must_use]
    pub fn get(&self, hunk: &Hunk) -> Option<HunkDigest> {
        let id = HunkId::of(hunk);
        self.entries
            .binary_search_by_key(&id, |entry| entry.id)
            .ok()
            .map(|at| self.entries[at].digest)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(crate) fn allocated_bytes(&self) -> u64 {
        u64::try_from(self.entries.capacity())
            .unwrap_or(u64::MAX)
            .saturating_mul(HUNK_DIGEST_ENTRY_BYTES)
    }

    pub(crate) fn spare_bytes(&self) -> u64 {
        let spare = self.entries.capacity().saturating_sub(self.entries.len());
        u64::try_from(spare)
            .unwrap_or(u64::MAX)
            .saturating_mul(HUNK_DIGEST_ENTRY_BYTES)
    }
}

/// BLAKE3 of a file range, truncated to 128 bits; `None` when the range is outside `bytes`.
pub(crate) fn digest_range(bytes: &[u8], range: &Range<u64>) -> Option<[u8; HUNK_DIGEST_BYTES]> {
    let start = usize::try_from(range.start).ok()?;
    let end = usize::try_from(range.end).ok()?;
    Some(digest(bytes.get(start..end)?))
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; HUNK_DIGEST_BYTES] {
    let hash = blake3::hash(bytes);
    let mut digest = [0u8; HUNK_DIGEST_BYTES];
    digest.copy_from_slice(&hash.as_bytes()[..HUNK_DIGEST_BYTES]);
    digest
}

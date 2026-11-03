use std::ops::Range;

use crate::model::Reason;
use crate::text::diff::Hunk;
use crate::text::digest::{
    HUNK_DIGEST_BYTES, HUNK_DIGEST_ENTRY_BYTES, HunkDigest, HunkDigests, digest,
};

use super::super::read::Lines;
use super::Charge;

pub(crate) fn hash_hunks(
    left: &Lines,
    right: &Lines,
    hunks: &[Hunk],
    charge: &mut Charge,
) -> Result<Option<(HunkDigests, Charge)>, Reason> {
    let count = u64::try_from(hunks.len()).map_err(|_| Reason::Unsupported)?;
    let bytes = count.saturating_mul(HUNK_DIGEST_ENTRY_BYTES);
    if !charge.try_add(bytes) {
        return Ok(None);
    }
    let mut charge = charge.take(bytes);
    let mut digests = HunkDigests::with_capacity(hunks.len());
    for hunk in hunks {
        let left = range_digest(left, &hunk.left_bytes)?;
        let right = range_digest(right, &hunk.right_bytes)?;
        digests.push(hunk, HunkDigest { left, right });
    }
    charge.settle(digests.allocated_bytes());
    Ok(Some((digests, charge)))
}

fn range_digest(lines: &Lines, range: &Range<u64>) -> Result<[u8; HUNK_DIGEST_BYTES], Reason> {
    let start = range
        .start
        .checked_sub(lines.base)
        .and_then(|at| usize::try_from(at).ok())
        .ok_or(Reason::ChangedOnDisk)?;
    let end = range
        .end
        .checked_sub(lines.base)
        .and_then(|at| usize::try_from(at).ok())
        .ok_or(Reason::ChangedOnDisk)?;
    let bytes = lines.bytes.get(start..end).ok_or(Reason::ChangedOnDisk)?;
    Ok(digest(bytes))
}

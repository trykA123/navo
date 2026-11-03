use std::ops::Range;

use navo_core::paging::{ByteSource, read_full};

use super::super::digest::{HUNK_DIGEST_BYTES, HunkDigests};
use super::{ApplyError, Direction, PickedHunk};

const HASH_BUFFER: usize = 64 * 1024;

pub(super) struct Stable<'a> {
    target: &'a dyn ByteSource,
    source: &'a dyn ByteSource,
    target_len: u64,
    source_len: u64,
    target_revision: u64,
    source_revision: u64,
}

impl<'a> Stable<'a> {
    pub(super) fn start(
        target: &'a dyn ByteSource,
        source: &'a dyn ByteSource,
    ) -> Result<Self, ApplyError> {
        let stable = Self {
            target,
            source,
            target_len: target.len(),
            source_len: source.len(),
            target_revision: target.revision(),
            source_revision: source.revision(),
        };
        stable.check()?;
        Ok(stable)
    }

    pub(super) fn finish(&self, result: Result<(), ApplyError>) -> Result<(), ApplyError> {
        self.check()?;
        result
    }

    pub(super) fn target(&self) -> &dyn ByteSource {
        self.target
    }

    pub(super) fn source(&self) -> &dyn ByteSource {
        self.source
    }

    pub(super) fn verify_hunks(
        &self,
        hunks: &[PickedHunk],
        digests: &HunkDigests,
        direction: Direction,
    ) -> Result<(), ApplyError> {
        let mut buffer = vec![0u8; HASH_BUFFER];
        for entry in hunks {
            let digest = digests.get(entry.hunk).ok_or(ApplyError::NotCopyable)?;
            let (target_hash, source_hash) = if direction.source_is_left() {
                (digest.right, digest.left)
            } else {
                (digest.left, digest.right)
            };
            let (target_range, source_range) = super::ranges(entry.hunk, direction);
            verify_range(self.source, source_range, source_hash, &mut buffer)?;
            verify_range(self.target, target_range, target_hash, &mut buffer)?;
        }
        Ok(())
    }

    fn check(&self) -> Result<(), ApplyError> {
        check_side(self.target, self.target_len, self.target_revision)?;
        check_side(self.source, self.source_len, self.source_revision)
    }
}

fn check_side(side: &dyn ByteSource, len: u64, revision: u64) -> Result<(), ApplyError> {
    let unchanged = side
        .revalidate()
        .map_err(|error| ApplyError::Read(format!("revalidating the file failed: {error}")))?;
    if !unchanged || side.revision() != revision || side.len() != len {
        return Err(ApplyError::Changed);
    }
    Ok(())
}

fn verify_range(
    side: &dyn ByteSource,
    range: Range<u64>,
    digest: [u8; HUNK_DIGEST_BYTES],
    buffer: &mut [u8],
) -> Result<(), ApplyError> {
    if range_hash(side, &range, buffer)? != digest {
        return Err(ApplyError::Changed);
    }
    Ok(())
}

fn range_hash(
    source: &dyn ByteSource,
    range: &Range<u64>,
    buffer: &mut [u8],
) -> Result<[u8; HUNK_DIGEST_BYTES], ApplyError> {
    let mut hasher = blake3::Hasher::new();
    let mut at = range.start;
    while at < range.end {
        let remaining = range.end.checked_sub(at).ok_or(ApplyError::Range)?;
        let capacity = u64::try_from(buffer.len()).map_err(|_| ApplyError::Range)?;
        let want = usize::try_from(remaining.min(capacity)).map_err(|_| ApplyError::Range)?;
        let read = read_full(source, at, &mut buffer[..want]).map_err(ApplyError::Read)?;
        if read != want {
            return Err(ApplyError::Read("the file ended early".to_owned()));
        }
        hasher.update(&buffer[..read]);
        at = at
            .checked_add(u64::try_from(read).map_err(|_| ApplyError::Range)?)
            .ok_or(ApplyError::Range)?;
    }
    let hash = hasher.finalize();
    let mut digest = [0u8; HUNK_DIGEST_BYTES];
    digest.copy_from_slice(&hash.as_bytes()[..HUNK_DIGEST_BYTES]);
    Ok(digest)
}

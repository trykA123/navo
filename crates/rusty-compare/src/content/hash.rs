use blake3::Hasher;

use super::{CONTENT_BLOCK, LARGE_CONTENT_THRESHOLD, PARALLEL_CONTENT_BLOCK};

pub(super) fn block_for(size: u64) -> usize {
    if size > LARGE_CONTENT_THRESHOLD {
        PARALLEL_CONTENT_BLOCK
    } else {
        CONTENT_BLOCK
    }
}

pub(super) fn update(hasher: &mut Hasher, buffer: &[u8], parallel: bool) {
    if parallel {
        hasher.update_rayon(buffer);
    } else {
        hasher.update(buffer);
    }
}

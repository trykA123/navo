//! The digest the diff screen shows for one change; the copy checks it before writing.
//!
//! Pure and host-free, so it moves with the converters to `sidewise-host` (G4).

use navo_compare::text::HunkDigest;

/// The 128-bit digest the UI shows for a change: BLAKE3 over both byte-range digests.
///
/// It covers both sides, so an edit to either side after the change was shown is caught.
pub(crate) fn shown_digest(digest: &HunkDigest) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&digest.left);
    hasher.update(&digest.right);
    let mut shown = [0u8; 16];
    shown.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    shown
}

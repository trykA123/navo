//! Record format v1 of the hash cache file and the digest that keys each record.
//! Pure functions over bytes; the file store builds on them.

use navo_compare::model::ContentKey;
use navo_core::{FileIdentityKind, ProviderId};

/// A 32-byte BLAKE3 digest, used for keys and for content hashes.
pub type Digest = [u8; 32];

/// File header: name plus format version 1.
pub const MAGIC: [u8; 8] = *b"SWHASH\x00\x01";
/// Bytes per record: key 32, hash 32, check 8.
pub const RECORD_LEN: usize = 72;
const CHECK_LEN: usize = 8;
const KEY_CONTEXT: &str = "sidewise 2026-11-18 compare hash cache key v1";
/// 1990-01-01 in ms: older stamps are shared by many files.
pub const OLDEST_MODIFIED_MS: i64 = 631_152_000_000;

/// Result of reading a whole cache file: good rows in file order plus damage flags.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub rows: Vec<(Digest, Digest)>,
    pub bad: usize,
    pub torn: bool,
    pub header_ok: bool,
}

/// Hashes a length-prefixed text (`u64` LE length, then bytes) into the key.
fn feed_text(hasher: &mut blake3::Hasher, text: &str) {
    hasher.update(&(text.len() as u64).to_le_bytes());
    hasher.update(text.as_bytes());
}

/// Builds the row key for a file, or `None` when the file may not be trusted.
///
/// No key for non-local providers, a missing or pre-1990 time, or a missing or
/// fingerprint identity (FAT, exFAT have no stable ids).
#[must_use]
pub fn key_digest(key: &ContentKey) -> Option<Digest> {
    if key.provider != ProviderId::local() {
        return None;
    }
    let modified = key.modified.filter(|ms| *ms >= OLDEST_MODIFIED_MS)?;
    let id = key.file_id?;
    let kind = match id.kind {
        FileIdentityKind::Native => 0u8,
        FileIdentityKind::WindowsIndex => 1,
        FileIdentityKind::Fingerprint { .. } => return None,
    };
    let mut hasher = blake3::Hasher::new_derive_key(KEY_CONTEXT);
    feed_text(&mut hasher, key.provider.as_str());
    feed_text(&mut hasher, &key.full_path.to_text());
    hasher.update(&key.size.to_le_bytes());
    hasher.update(&modified.to_le_bytes());
    hasher.update(&id.volume.to_le_bytes());
    hasher.update(&id.file);
    hasher.update(&[kind]);
    Some(*hasher.finalize().as_bytes())
}

/// First 8 bytes of `blake3(key ‖ hash)`, the record's integrity check.
fn check(key: &Digest, hash: &Digest) -> [u8; CHECK_LEN] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(key);
    hasher.update(hash);
    let mut out = [0; CHECK_LEN];
    out.copy_from_slice(&hasher.finalize().as_bytes()[..CHECK_LEN]);
    out
}

/// Encodes one 72-byte record: key, hash, check.
#[must_use]
pub fn encode(key: &Digest, hash: &Digest) -> [u8; RECORD_LEN] {
    let mut out = [0; RECORD_LEN];
    out[..32].copy_from_slice(key);
    out[32..64].copy_from_slice(hash);
    out[64..].copy_from_slice(&check(key, hash));
    out
}

/// Decodes one record into `(key, hash)`; `None` when the check does not match.
#[must_use]
pub fn decode(record: &[u8; RECORD_LEN]) -> Option<(Digest, Digest)> {
    let mut key = [0; 32];
    let mut hash = [0; 32];
    key.copy_from_slice(&record[..32]);
    hash.copy_from_slice(&record[32..64]);
    (record[64..] == check(&key, &hash)).then_some((key, hash))
}

/// Reads a whole file image: empty is fine, a wrong header drops everything,
/// bad records are counted and skipped, and a short tail is flagged as torn.
#[must_use]
pub fn replay(bytes: &[u8]) -> Replay {
    if bytes.is_empty() {
        return Replay {
            header_ok: true,
            ..Replay::default()
        };
    }
    if bytes.len() < MAGIC.len() || bytes[..MAGIC.len()] != MAGIC {
        return Replay::default();
    }
    let body = &bytes[MAGIC.len()..];
    let mut out = Replay {
        header_ok: true,
        torn: !body.len().is_multiple_of(RECORD_LEN),
        ..Replay::default()
    };
    for record in body.as_chunks::<RECORD_LEN>().0 {
        match decode(record) {
            Some(row) => out.rows.push(row),
            None => out.bad += 1,
        }
    }
    out
}

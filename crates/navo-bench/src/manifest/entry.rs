use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::source::{EntryKind, ReadOnlySource, Record, RecordId};

pub(super) type Hash = [u8; 32];

/// One walked entry as hashes and numbers; no name or path survives.
#[derive(Clone, Copy, Debug)]
pub(super) struct Entry {
    pub key: Hash,
    pub kind: u8,
    pub size: u64,
    /// Nanoseconds since the Unix epoch, saturating.
    pub mtime: Option<i64>,
    pub attributes: u32,
    pub hard_links: Option<u64>,
    /// File content hash, or the hash of a link's raw target text.
    pub payload: Option<Hash>,
}

/// What a renamed entry keeps: everything but its path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Signature {
    kind: u8,
    size: u64,
    mtime: Option<i64>,
    payload: Option<Hash>,
}

impl Entry {
    pub(super) fn from_record(record: &Record, key: Hash, payload: Option<Hash>) -> Self {
        Self {
            key,
            kind: kind_code(record.kind),
            size: match record.kind {
                EntryKind::Directory => 0,
                _ => record.size,
            },
            mtime: record.mtime.map(nanos),
            attributes: record.attributes,
            hard_links: record.hard_links,
            payload,
        }
    }

    pub(super) fn signature(&self) -> Signature {
        Signature {
            kind: self.kind,
            size: self.size,
            mtime: self.mtime,
            payload: self.payload,
        }
    }

    pub(super) fn digest(&self) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.key);
        hasher.update(&[self.kind]);
        hasher.update(&self.size.to_le_bytes());
        put_option(&mut hasher, self.mtime.map(i64::to_le_bytes));
        hasher.update(&self.attributes.to_le_bytes());
        put_option(&mut hasher, self.hard_links.map(u64::to_le_bytes));
        put_option(&mut hasher, self.payload);
        *hasher.finalize().as_bytes()
    }
}

fn put_option<const N: usize>(hasher: &mut blake3::Hasher, value: Option<[u8; N]>) {
    match value {
        Some(bytes) => hasher.update(&[1]).update(&bytes),
        None => hasher.update(&[0]),
    };
}

pub(super) fn is_link_kind(kind: EntryKind) -> bool {
    matches!(
        kind,
        EntryKind::SymbolicLink | EntryKind::Junction | EntryKind::ReparsePoint
    )
}

pub(super) fn kind_code(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::File => 0,
        EntryKind::Directory => 1,
        EntryKind::SymbolicLink => 2,
        EntryKind::Junction => 3,
        EntryKind::ReparsePoint => 4,
        EntryKind::Other => 5,
    }
}

fn nanos(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
        Err(error) => i64::try_from(error.duration().as_nanos()).map_or(i64::MIN, |value| -value),
    }
}

/// Hashes the path component by component, each length-prefixed.
pub(super) fn path_key(path: &Path) -> Hash {
    let mut hasher = blake3::Hasher::new();
    for part in path.components() {
        put_os(&mut hasher, part.as_os_str());
    }
    *hasher.finalize().as_bytes()
}

pub(super) fn text_hash(text: &OsStr) -> Hash {
    let mut hasher = blake3::Hasher::new();
    put_os(&mut hasher, text);
    *hasher.finalize().as_bytes()
}

#[cfg(unix)]
fn put_os(hasher: &mut blake3::Hasher, text: &OsStr) {
    use std::os::unix::ffi::OsStrExt;
    let bytes = text.as_bytes();
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(windows)]
fn put_os(hasher: &mut blake3::Hasher, text: &OsStr) {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u8> = text.encode_wide().flat_map(u16::to_le_bytes).collect();
    hasher.update(&(units.len() as u64).to_le_bytes());
    hasher.update(&units);
}

/// Hashes at most `limit` bytes; `None` when unreadable or longer than `limit`.
pub(super) fn hash_file(source: &ReadOnlySource, id: &RecordId, limit: u64) -> Option<(Hash, u64)> {
    let file = source.open_read(id).ok()?;
    let mut reader = file.take(limit.saturating_add(1));
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = reader.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total.saturating_add(read as u64);
    }
    (total <= limit).then(|| (*hasher.finalize().as_bytes(), total))
}

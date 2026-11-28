//! Binary framing, BLAKE3 integrity validation, and safe path keying for editor drafts.

use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf};

use super::{DRAFT_MAGIC, DRAFT_VERSION, DraftError, DraftFileStamp, DraftRecord, MAX_DRAFT_BYTES};

/// Fixed binary header size: 16-byte magic + u16 version + u32 meta_len + u32 content_len + 32-byte BLAKE3.
pub(super) const HEADER_LEN: usize = 16 + 2 + 4 + 4 + 32;

/// Upper bound on encoded metadata bytes (64 KiB) to bound allocations on untrusted input.
pub(super) const MAX_META_BYTES: usize = 64 * 1024;

/// Maximum on-disk draft file size accepted by [`read_record_bounded`].
pub(super) const MAX_RECORD_BYTES: u64 = (HEADER_LEN + MAX_META_BYTES + MAX_DRAFT_BYTES) as u64;

const KEY_DERIVE_CONTEXT: &str = "navo.editor.draft.key.v1";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DraftMeta {
    target_path: String,
    file_stamp: Option<DraftFileStamp>,
    saved_at_unix_secs: i64,
    content_blake3: [u8; 32],
}

struct Header {
    version: u16,
    meta_len: usize,
    content_len: usize,
    checksum: [u8; 32],
}

/// Computes a deterministic 64-character lowercase hex BLAKE3 key from a lexically normalized path.
pub(super) fn canonical_key(target: &Path) -> String {
    let normalized = normalize_lexical_bytes(target);
    let mut hasher = blake3::Hasher::new_derive_key(KEY_DERIVE_CONTEXT);
    hasher.update(&normalized);
    hasher.finalize().to_hex().to_string()
}

/// Returns `true` if `stem` is a valid 64-character lowercase hex draft key.
pub(super) fn is_valid_key(stem: &str) -> bool {
    stem.len() == 64 && stem.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Serializes metadata and raw content bytes into the versioned, BLAKE3-checksummed draft format.
pub(super) fn encode_record(
    target: &Path,
    content: &[u8],
    file_stamp: Option<DraftFileStamp>,
    saved_at_unix_secs: i64,
) -> Result<Vec<u8>, DraftError> {
    let content_blake3 = *blake3::hash(content).as_bytes();
    let meta = DraftMeta {
        target_path: target.to_string_lossy().into_owned(),
        file_stamp,
        saved_at_unix_secs,
        content_blake3,
    };
    let meta_bytes =
        postcard::to_allocvec(&meta).map_err(|e| DraftError::CorruptMeta(e.to_string()))?;
    if meta_bytes.len() > MAX_META_BYTES {
        return Err(DraftError::CorruptMeta(format!(
            "metadata length {} exceeds {MAX_META_BYTES} bytes",
            meta_bytes.len()
        )));
    }
    let meta_len = u32::try_from(meta_bytes.len())
        .map_err(|_| DraftError::CorruptMeta("metadata length overflow".to_owned()))?;
    let content_len = u32::try_from(content.len())
        .map_err(|_| DraftError::CorruptMeta("content length overflow".to_owned()))?;
    let checksum = compute_checksum(DRAFT_VERSION, meta_len, content_len, &meta_bytes, content);

    let mut out = Vec::with_capacity(HEADER_LEN + meta_bytes.len() + content.len());
    out.extend_from_slice(DRAFT_MAGIC);
    out.extend_from_slice(&DRAFT_VERSION.to_le_bytes());
    out.extend_from_slice(&meta_len.to_le_bytes());
    out.extend_from_slice(&content_len.to_le_bytes());
    out.extend_from_slice(&checksum);
    out.extend_from_slice(&meta_bytes);
    out.extend_from_slice(content);
    Ok(out)
}

/// Reads and validates a draft file from disk with strict size bounds to prevent OOM.
pub(super) fn read_record_bounded(
    key: String,
    path: &Path,
) -> Result<Option<DraftRecord>, DraftError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DraftError::Io(error)),
    };
    let file_len = file.metadata()?.len();
    if file_len > MAX_RECORD_BYTES {
        return Err(DraftError::TooLarge {
            found: file_len,
            limit: MAX_RECORD_BYTES,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(file_len).unwrap_or(0));
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(DraftError::TooLarge {
            found: bytes.len() as u64,
            limit: MAX_RECORD_BYTES,
        });
    }
    decode_record(key, &bytes).map(Some)
}

/// Decodes and validates a complete in-memory draft record buffer.
pub(super) fn decode_record(key: String, bytes: &[u8]) -> Result<DraftRecord, DraftError> {
    let header = parse_header(bytes)?;
    let expected_len = HEADER_LEN
        .checked_add(header.meta_len)
        .and_then(|n| n.checked_add(header.content_len))
        .ok_or_else(|| DraftError::Truncated("record length overflow".to_owned()))?;
    if bytes.len() != expected_len {
        return Err(DraftError::Truncated(format!(
            "file is {} bytes, header expects {expected_len}",
            bytes.len()
        )));
    }
    let meta_start = HEADER_LEN;
    let content_start = meta_start + header.meta_len;
    let meta_bytes = &bytes[meta_start..content_start];
    let content_bytes = &bytes[content_start..];

    let meta_len_u32 = u32::try_from(header.meta_len)
        .map_err(|_| DraftError::Truncated("metadata length overflow".to_owned()))?;
    let content_len_u32 = u32::try_from(header.content_len)
        .map_err(|_| DraftError::Truncated("content length overflow".to_owned()))?;
    let actual_checksum = compute_checksum(
        header.version,
        meta_len_u32,
        content_len_u32,
        meta_bytes,
        content_bytes,
    );
    if actual_checksum != header.checksum {
        return Err(DraftError::BadChecksum);
    }

    let meta: DraftMeta =
        postcard::from_bytes(meta_bytes).map_err(|e| DraftError::CorruptMeta(e.to_string()))?;
    if *blake3::hash(content_bytes).as_bytes() != meta.content_blake3 {
        return Err(DraftError::BadChecksum);
    }
    Ok(DraftRecord {
        key,
        target_path: PathBuf::from(meta.target_path),
        file_stamp: meta.file_stamp,
        saved_at_unix_secs: meta.saved_at_unix_secs,
        content_blake3: meta.content_blake3,
        content: content_bytes.to_vec(),
    })
}

fn parse_header(bytes: &[u8]) -> Result<Header, DraftError> {
    if bytes.len() < DRAFT_MAGIC.len() || &bytes[..DRAFT_MAGIC.len()] != DRAFT_MAGIC {
        return Err(DraftError::BadMagic);
    }
    if bytes.len() < HEADER_LEN {
        return Err(DraftError::Truncated(format!(
            "header is {} bytes, expected at least {HEADER_LEN}",
            bytes.len()
        )));
    }
    let rest = &bytes[DRAFT_MAGIC.len()..HEADER_LEN];
    let version = u16::from_le_bytes([rest[0], rest[1]]);
    if version == 0 {
        return Err(DraftError::BadMagic);
    }
    if version > DRAFT_VERSION {
        return Err(DraftError::UnsupportedVersion {
            found: version,
            supported: DRAFT_VERSION,
        });
    }
    let meta_len = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
    let content_len = u32::from_le_bytes([rest[6], rest[7], rest[8], rest[9]]) as usize;
    if meta_len > MAX_META_BYTES {
        return Err(DraftError::CorruptMeta(format!(
            "header metadata length {meta_len} exceeds {MAX_META_BYTES}"
        )));
    }
    if content_len > MAX_DRAFT_BYTES {
        return Err(DraftError::TooLarge {
            found: content_len as u64,
            limit: MAX_DRAFT_BYTES as u64,
        });
    }
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&rest[10..42]);
    Ok(Header {
        version,
        meta_len,
        content_len,
        checksum,
    })
}

fn compute_checksum(
    version: u16,
    meta_len: u32,
    content_len: u32,
    meta_bytes: &[u8],
    content_bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&version.to_le_bytes());
    hasher.update(&meta_len.to_le_bytes());
    hasher.update(&content_len.to_le_bytes());
    hasher.update(meta_bytes);
    hasher.update(content_bytes);
    *hasher.finalize().as_bytes()
}

/// Lexically normalizes a path into stable bytes without touching the filesystem.
fn normalize_lexical_bytes(target: &Path) -> Vec<u8> {
    let mut stack: Vec<Vec<u8>> = Vec::new();
    for component in target.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match stack.last().map(Vec::as_slice) {
                Some(b"..") | None => stack.push(b"..".to_vec()),
                Some(b"/") => {}
                Some(_) => {
                    stack.pop();
                }
            },
            Component::RootDir => stack.push(b"/".to_vec()),
            Component::Prefix(prefix) => {
                stack.push(prefix.as_os_str().as_encoded_bytes().to_vec());
            }
            Component::Normal(part) => {
                stack.push(part.as_encoded_bytes().to_vec());
            }
        }
    }
    let mut out = Vec::new();
    for (idx, part) in stack.iter().enumerate() {
        if idx > 0 {
            out.push(0);
        }
        out.extend_from_slice(part);
    }
    out
}

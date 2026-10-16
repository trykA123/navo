use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusty_core::ProviderPath;
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::{CacheKey, CachedListing};

const MAGIC: &[u8; 2] = b"FL";
pub(crate) const FORMAT: u8 = 1;
#[cfg(test)]
pub(crate) const FORMAT_AT: usize = MAGIC.len();
// No listing that fits a 32 MB cache inflates past this; a larger length is damage.
const MAX_PLAIN: usize = 256 * 1024 * 1024;
const MIN_ROW: usize = 1 + 1 + 8 + 1 + 1 + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DecodeError {
    #[error("not a listing row")]
    Header,
    #[error("listing format {0} is not {FORMAT}")]
    Format(u8),
    #[error("checksum mismatch")]
    Checksum,
    #[error("lz4 decompression failed")]
    Decompress,
    #[error("row body is malformed")]
    Body,
}

pub(crate) fn encode_key(key: &CacheKey) -> Vec<u8> {
    let location = key.location.as_str().as_bytes();
    let mut out = Vec::with_capacity(9 + location.len());
    match key.volume_serial {
        Some(serial) => {
            out.push(1);
            out.extend_from_slice(&serial.to_le_bytes());
        }
        None => out.push(0),
    }
    out.extend_from_slice(location);
    out
}

pub(crate) struct Body {
    plain: Vec<u8>,
    rows_at: usize,
}

impl Body {
    pub(crate) fn new(listing: &CachedListing) -> Self {
        let mut plain = Vec::with_capacity(32 + listing.rows.len() * 40);
        plain.extend_from_slice(&millis(listing.read_at).to_le_bytes());
        match listing.folder_changed {
            Some(at) => {
                plain.push(1);
                plain.extend_from_slice(&millis(at).to_le_bytes());
            }
            None => plain.push(0),
        }
        plain.push(u8::from(listing.complete));
        let rows_at = plain.len();
        let count = u32::try_from(listing.rows.len()).unwrap_or(u32::MAX);
        plain.extend_from_slice(&count.to_le_bytes());
        listing
            .rows
            .iter()
            .for_each(|row| encode_row(&mut plain, row));
        Self { plain, rows_at }
    }

    pub(crate) fn rows_digest(&self) -> u32 {
        crc32fast::hash(&self.plain[self.rows_at..])
    }

    pub(crate) fn pack(&self) -> Vec<u8> {
        let packed = lz4_flex::block::compress(&self.plain);
        let plain_len = u32::try_from(self.plain.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(MAGIC.len() + 9 + packed.len());
        out.extend_from_slice(MAGIC);
        out.push(FORMAT);
        out.extend_from_slice(&plain_len.to_le_bytes());
        out.extend_from_slice(&crc32fast::hash(&packed).to_le_bytes());
        out.extend_from_slice(&packed);
        out
    }
}

#[cfg(test)]
pub(crate) fn encode(listing: &CachedListing) -> Vec<u8> {
    Body::new(listing).pack()
}

pub(crate) fn decode(bytes: &[u8]) -> Result<CachedListing, DecodeError> {
    let mut header = Reader(bytes);
    let magic = header.take(MAGIC.len()).map_err(|_| DecodeError::Header)?;
    if magic != MAGIC {
        return Err(DecodeError::Header);
    }
    let format = header.u8().map_err(|_| DecodeError::Header)?;
    if format != FORMAT {
        return Err(DecodeError::Format(format));
    }
    let plain_len = header.u32().map_err(|_| DecodeError::Header)?;
    let checksum = header.u32().map_err(|_| DecodeError::Header)?;
    let packed = header.0;
    if crc32fast::hash(packed) != checksum {
        return Err(DecodeError::Checksum);
    }
    let plain_len = usize::try_from(plain_len)
        .ok()
        .filter(|len| *len <= MAX_PLAIN)
        .ok_or(DecodeError::Header)?;
    let plain =
        lz4_flex::block::decompress(packed, plain_len).map_err(|_| DecodeError::Decompress)?;
    decode_body(&plain)
}

fn encode_row(out: &mut Vec<u8>, row: &FsEntry) {
    out.push(match row.kind {
        EntryKind::File => 0,
        EntryKind::Folder => 1,
        EntryKind::Link => 2,
    });
    out.push(attribute_bits(row.attributes));
    out.extend_from_slice(&row.size.to_le_bytes());
    match row.modified_ms {
        Some(ms) => {
            out.push(1);
            out.extend_from_slice(&ms.to_le_bytes());
        }
        None => out.push(0),
    }
    let (tag, name) = name_bytes(&row.name);
    out.push(tag);
    let len = u32::try_from(name.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&name);
}

fn name_bytes(name: &ProviderPath) -> (u8, Vec<u8>) {
    if let Some(text) = name.as_str() {
        return (0, text.as_bytes().to_vec());
    }
    match name.to_wide() {
        Some(units) => (
            1,
            units.iter().flat_map(|unit| unit.to_le_bytes()).collect(),
        ),
        None => (2, name.to_bytes().unwrap_or_default()),
    }
}

fn attribute_bits(attributes: Attributes) -> u8 {
    u8::from(attributes.hidden)
        | u8::from(attributes.system) << 1
        | u8::from(attributes.readonly) << 2
        | u8::from(attributes.link_to_folder) << 3
}

fn decode_body(plain: &[u8]) -> Result<CachedListing, DecodeError> {
    let mut reader = Reader(plain);
    let read_at = from_millis(reader.u64()?);
    let folder_changed = match reader.u8()? {
        0 => None,
        1 => Some(from_millis(reader.u64()?)),
        _ => return Err(DecodeError::Body),
    };
    let complete = reader.flag()?;
    let count = usize::try_from(reader.u32()?).map_err(|_| DecodeError::Body)?;
    let mut rows = Vec::with_capacity(count.min(plain.len() / MIN_ROW));
    for _ in 0..count {
        rows.push(decode_row(&mut reader)?);
    }
    if !reader.0.is_empty() {
        return Err(DecodeError::Body);
    }
    Ok(CachedListing {
        rows: rows.into(),
        read_at,
        folder_changed,
        complete,
    })
}

fn decode_row(reader: &mut Reader<'_>) -> Result<FsEntry, DecodeError> {
    let kind = match reader.u8()? {
        0 => EntryKind::File,
        1 => EntryKind::Folder,
        2 => EntryKind::Link,
        _ => return Err(DecodeError::Body),
    };
    let bits = reader.u8()?;
    if bits > 0b1111 {
        return Err(DecodeError::Body);
    }
    let size = reader.u64()?;
    let modified_ms = match reader.u8()? {
        0 => None,
        1 => Some(i64::from_le_bytes(reader.array()?)),
        _ => return Err(DecodeError::Body),
    };
    let tag = reader.u8()?;
    let len = usize::try_from(reader.u32()?).map_err(|_| DecodeError::Body)?;
    let name = decode_name(tag, reader.take(len)?)?;
    Ok(FsEntry {
        name,
        kind,
        size,
        modified_ms,
        attributes: Attributes {
            hidden: bits & 1 != 0,
            system: bits & 2 != 0,
            readonly: bits & 4 != 0,
            link_to_folder: bits & 8 != 0,
        },
    })
}

fn decode_name(tag: u8, bytes: &[u8]) -> Result<ProviderPath, DecodeError> {
    match tag {
        0 if std::str::from_utf8(bytes).is_ok() => Ok(ProviderPath::from_bytes(bytes)),
        1 if bytes.len().is_multiple_of(2) => {
            let (pairs, _) = bytes.as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().copied().map(u16::from_le_bytes).collect();
            Ok(ProviderPath::from_wide(&units))
        }
        2 => Ok(ProviderPath::from_bytes(bytes)),
        _ => Err(DecodeError::Body),
    }
}

fn millis(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    })
}

fn from_millis(ms: u64) -> SystemTime {
    UNIX_EPOCH
        .checked_add(Duration::from_millis(ms))
        .unwrap_or(UNIX_EPOCH)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let (head, rest) = self.0.split_at_checked(len).ok_or(DecodeError::Body)?;
        self.0 = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.take(N)?.try_into().map_err(|_| DecodeError::Body)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        self.array::<1>().map(|[byte]| byte)
    }

    fn flag(&mut self) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::Body),
        }
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        self.array().map(u64::from_le_bytes)
    }
}

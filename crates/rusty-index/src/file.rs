use std::{fs::File, io::Read as _, path::Path};

use crate::table::NameTable;
use crate::usn::FrnMap;

pub const FORMAT_VERSION: u16 = 2;
const MAGIC: &[u8; 16] = b"FERRITE-INDEX\0\0\0";
const HEADER_LEN: usize = 70;
const CHECKSUM_START: usize = HEADER_LEN - 4;
const MAX_ENTRIES: u32 = 64_000_000;
const MAX_NAME_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub volume_serial: u64,
    pub journal_id: u64,
    pub next_usn: i64,
    pub root: String,
    pub built_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("cannot allocate index snapshot: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
    #[error("not a Ferrite index file")]
    BadMagic,
    #[error("index format {found} differs from supported format {supported}")]
    UnsupportedVersion { found: u16, supported: u16 },
    #[error("index file length does not match its header")]
    BadLength,
    #[error("index checksum does not match")]
    BadChecksum,
    #[error("invalid index file: {0}")]
    InvalidData(&'static str),
}

pub fn save(path: &Path, header: &Header, table: &NameTable) -> Result<(), FileError> {
    save_with_ids(path, header, table, &FrnMap::new())
}

pub fn save_with_ids(
    path: &Path,
    header: &Header,
    table: &NameTable,
    ids: &FrnMap,
) -> Result<(), FileError> {
    validate_ids(table, ids)?;
    let lengths = Lengths {
        root: u32::try_from(header.root.len()).map_err(|_| FileError::BadLength)?,
        names: u32::try_from(table.raw_parts().0.len()).map_err(|_| FileError::BadLength)?,
        entries: table.slots(),
        references: u32::try_from(ids.len()).map_err(|_| FileError::BadLength)?,
    };
    let capacity = lengths
        .body_bytes()?
        .checked_add(HEADER_LEN)
        .ok_or(FileError::BadLength)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity)?;
    encode_header(&mut bytes, header, &lengths);
    encode_table(&mut bytes, table);
    for (&reference, &id) in ids {
        bytes.extend_from_slice(&reference.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    let checksum = checksum(&bytes[..CHECKSUM_START], &bytes[HEADER_LEN..]);
    bytes[CHECKSUM_START..HEADER_LEN].copy_from_slice(&checksum.to_le_bytes());
    rusty_store::write_atomic(path, &bytes)?;
    Ok(())
}

fn encode_header(bytes: &mut Vec<u8>, header: &Header, lengths: &Lengths) {
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&header.volume_serial.to_le_bytes());
    bytes.extend_from_slice(&header.journal_id.to_le_bytes());
    bytes.extend_from_slice(&header.next_usn.to_le_bytes());
    bytes.extend_from_slice(&header.built_at.to_le_bytes());
    bytes.extend_from_slice(&lengths.root.to_le_bytes());
    bytes.extend_from_slice(&lengths.names.to_le_bytes());
    bytes.extend_from_slice(&lengths.entries.to_le_bytes());
    bytes.extend_from_slice(&lengths.references.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(header.root.as_bytes());
}

fn encode_table(bytes: &mut Vec<u8>, table: &NameTable) {
    let (names, offsets, parents, flags) = table.raw_parts();
    bytes.extend_from_slice(names.as_bytes());
    for &offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for &parent in parents {
        bytes.extend_from_slice(&parent.to_le_bytes());
    }
    for &flag in flags {
        bytes.extend_from_slice(&flag.to_le_bytes());
    }
}

pub fn load(path: &Path) -> Result<(Header, NameTable), FileError> {
    let (header, table, _) = load_with_ids(path)?;
    Ok((header, table))
}

pub fn load_with_ids(path: &Path) -> Result<(Header, NameTable, FrnMap), FileError> {
    let mut file = File::open(path)?;
    let mut prefix = [0; HEADER_LEN];
    file.read_exact(&mut prefix)?;
    let (mut header, lengths, expected_checksum) = decode_header(&prefix)?;
    let body_len = lengths.body_bytes()?;
    if file.metadata()?.len() != HEADER_LEN as u64 + body_len as u64 {
        return Err(FileError::BadLength);
    }
    let mut body = Vec::new();
    body.try_reserve_exact(body_len)?;
    body.resize(body_len, 0);
    file.read_exact(&mut body)?;
    if checksum(&prefix[..CHECKSUM_START], &body) != expected_checksum {
        return Err(FileError::BadChecksum);
    }
    let mut input = body.as_slice();
    header.root = read_string(&mut input, lengths.root)?;
    let names = read_string(&mut input, lengths.names)?;
    let offsets = read_values(&mut input, lengths.entries, u32::from_le_bytes)?;
    let parents = read_values(&mut input, lengths.entries, u32::from_le_bytes)?;
    let flags = read_values(&mut input, lengths.entries, u16::from_le_bytes)?;
    let table = NameTable::from_raw_parts(names, offsets, parents, flags)
        .ok_or(FileError::InvalidData("invalid name table"))?;
    validate_root(&header, &table)?;
    let ids = decode_ids(&mut input, lengths.references)?;
    validate_ids(&table, &ids)?;
    Ok((header, table, ids))
}

fn decode_ids(input: &mut &[u8], count: u32) -> Result<FrnMap, FileError> {
    let mut ids = FrnMap::new();
    ids.try_reserve(count as usize)?;
    for _ in 0..count {
        let reference = u128::from_le_bytes(read_array(input)?);
        let id = u32::from_le_bytes(read_array(input)?);
        if ids.insert(reference, id).is_some() {
            return Err(FileError::InvalidData("duplicate file reference"));
        }
    }
    Ok(ids)
}

pub(crate) fn validate_ids(table: &NameTable, ids: &FrnMap) -> Result<(), FileError> {
    if ids.len() > table.slots() as usize || ids.values().any(|&id| table.flags(id).is_none()) {
        return Err(FileError::InvalidData(
            "file reference does not name a live entry",
        ));
    }
    Ok(())
}

pub(crate) fn validate_root(header: &Header, table: &NameTable) -> Result<(), FileError> {
    let root = header
        .root
        .split_once("://")
        .map_or(header.root.as_str(), |(_, path)| path);
    let windows = root.starts_with(r"\\")
        || root.as_bytes().get(1) == Some(&b':')
            && root.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    let root = if windows {
        root.trim_end_matches(['\\', '/'])
    } else {
        root.trim_end_matches('/')
    };
    if table.slots() != 0 && table.name(0) != Some(root) {
        return Err(FileError::InvalidData(
            "name table root differs from header",
        ));
    }
    Ok(())
}

struct Lengths {
    root: u32,
    names: u32,
    entries: u32,
    references: u32,
}

impl Lengths {
    fn body_bytes(&self) -> Result<usize, FileError> {
        if self.entries > MAX_ENTRIES
            || u64::from(self.names) > MAX_NAME_BYTES
            || self.references > self.entries
        {
            return Err(FileError::InvalidData("snapshot section exceeds limits"));
        }
        let bytes = u64::from(self.root)
            + u64::from(self.names)
            + u64::from(self.entries) * 10
            + u64::from(self.references) * 20;
        usize::try_from(bytes).map_err(|_| FileError::BadLength)
    }
}

fn decode_header(prefix: &[u8]) -> Result<(Header, Lengths, u32), FileError> {
    let mut input = prefix;
    let magic = read_array::<16>(&mut input)?;
    let version = u16::from_le_bytes(read_array(&mut input)?);
    let header = Header {
        volume_serial: u64::from_le_bytes(read_array(&mut input)?),
        journal_id: u64::from_le_bytes(read_array(&mut input)?),
        next_usn: i64::from_le_bytes(read_array(&mut input)?),
        built_at: i64::from_le_bytes(read_array(&mut input)?),
        root: String::new(),
    };
    let lengths = Lengths {
        root: u32::from_le_bytes(read_array(&mut input)?),
        names: u32::from_le_bytes(read_array(&mut input)?),
        entries: u32::from_le_bytes(read_array(&mut input)?),
        references: u32::from_le_bytes(read_array(&mut input)?),
    };
    let checksum = u32::from_le_bytes(read_array(&mut input)?);
    if &magic != MAGIC {
        return Err(FileError::BadMagic);
    }
    if version != FORMAT_VERSION {
        return Err(FileError::UnsupportedVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    Ok((header, lengths, checksum))
}

fn checksum(prefix: &[u8], body: &[u8]) -> u32 {
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(prefix);
    checksum.update(body);
    checksum.finalize()
}

fn take<'a>(input: &mut &'a [u8], length: usize) -> Result<&'a [u8], FileError> {
    let (head, rest) = input.split_at_checked(length).ok_or(FileError::BadLength)?;
    *input = rest;
    Ok(head)
}

fn read_array<const N: usize>(input: &mut &[u8]) -> Result<[u8; N], FileError> {
    take(input, N)?.try_into().map_err(|_| FileError::BadLength)
}

fn read_values<const N: usize, T>(
    input: &mut &[u8],
    count: u32,
    decode: fn([u8; N]) -> T,
) -> Result<Vec<T>, FileError> {
    let mut values = Vec::new();
    values.try_reserve_exact(count as usize)?;
    for _ in 0..count {
        values.push(decode(read_array(input)?));
    }
    Ok(values)
}

fn read_string(input: &mut &[u8], count: u32) -> Result<String, FileError> {
    let value = std::str::from_utf8(take(input, count as usize)?)
        .map_err(|_| FileError::InvalidData("index text is not UTF-8"))?;
    let mut text = String::new();
    text.try_reserve_exact(value.len())?;
    text.push_str(value);
    Ok(text)
}

#[cfg(test)]
mod tests;

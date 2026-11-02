use std::io::{Read, Seek, SeekFrom};

use super::{Error, MAX_CONTAINER, MAX_HEADER, Result, read_exact};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlfHeader {
    pub size: u32,
    pub object_count: u32,
    pub start: SystemTime,
    pub end: SystemTime,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ObjectHeader {
    pub size: usize,
    pub header_size: usize,
    pub version: u16,
    pub kind: u32,
}

pub(super) fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(field(bytes, at)?))
}

pub(super) fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(field(bytes, at)?))
}

fn field<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N]> {
    at.checked_add(N)
        .and_then(|end| bytes.get(at..end))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Invalid("truncated BLF field"))
}

fn system_time(bytes: &[u8]) -> Result<SystemTime> {
    Ok(SystemTime {
        year: u16_at(bytes, 0)?,
        month: u16_at(bytes, 2)?,
        day_of_week: u16_at(bytes, 4)?,
        day: u16_at(bytes, 6)?,
        hour: u16_at(bytes, 8)?,
        minute: u16_at(bytes, 10)?,
        second: u16_at(bytes, 12)?,
        milliseconds: u16_at(bytes, 14)?,
    })
}

impl BlfHeader {
    pub(super) fn read(source: &mut (impl Read + Seek), end: u64) -> Result<Self> {
        source.seek(SeekFrom::Start(0))?;
        let mut bytes = [0; 72];
        read_exact(source, &mut bytes)?;
        if !bytes.starts_with(b"LOGG") {
            return Err(Error::Invalid("missing LOGG signature"));
        }
        let size = u32_at(&bytes, 4)?;
        if !(72..=MAX_HEADER as u32).contains(&size) || u64::from(size) > end {
            return Err(Error::Invalid("invalid BLF file header size"));
        }
        Ok(Self {
            size,
            object_count: u32_at(&bytes, 32)?,
            start: system_time(&bytes[40..56])?,
            end: system_time(&bytes[56..72])?,
        })
    }
}

impl ObjectHeader {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if !bytes.starts_with(b"LOBJ") {
            return Err(Error::Invalid("missing LOBJ signature"));
        }
        let head = Self {
            header_size: usize::from(u16_at(bytes, 4)?),
            version: u16_at(bytes, 6)?,
            size: u32_at(bytes, 8)? as usize,
            kind: u32_at(bytes, 12)?,
        };
        if !(16..=MAX_HEADER).contains(&head.header_size) || head.size < head.header_size {
            return Err(Error::Invalid("invalid BLF object header size"));
        }
        if head.size > MAX_CONTAINER + MAX_HEADER {
            return Err(Error::Invalid("BLF object exceeds the size limit"));
        }
        Ok(head)
    }

    pub(super) fn time_ns(self, bytes: &[u8]) -> Result<u64> {
        let minimum = match self.version {
            1 => 32,
            2 => 40,
            _ => return Err(Error::Invalid("unsupported BLF object header version")),
        };
        if self.header_size < minimum {
            return Err(Error::Invalid("short BLF timestamp header"));
        }
        let timestamp = u64::from_le_bytes(field(bytes, 24)?);
        match u32_at(bytes, 16)? & 3 {
            1 => timestamp
                .checked_mul(10_000)
                .ok_or(Error::Invalid("BLF timestamp overflow")),
            2 => Ok(timestamp),
            _ => Err(Error::Invalid("invalid BLF timestamp units")),
        }
    }
}

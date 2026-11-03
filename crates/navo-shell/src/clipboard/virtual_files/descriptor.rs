use std::time::{Duration, SystemTime};

use super::names::{BadName, relative_path};
use super::{VirtualEntry, VirtualError};

const FD_ATTRIBUTES: u32 = 0x4;
const FD_WRITESTIME: u32 = 0x20;
const FD_FILESIZE: u32 = 0x40;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const NAME_UNITS: usize = 260;
const FIXED: usize = 72;
/// Seconds between 1601-01-01 (FILETIME) and 1970-01-01.
const EPOCH_GAP: u64 = 11_644_473_600;

/// How the names in a descriptor are stored.
pub(super) enum Names<'a> {
    /// FILEGROUPDESCRIPTORW: UTF-16.
    Wide,
    /// FILEGROUPDESCRIPTORA: the system code page, decoded by the caller.
    Ansi(&'a dyn Fn(&[u8]) -> Option<String>),
}

impl Names<'_> {
    fn unit(&self) -> usize {
        match self {
            Self::Wide => 2,
            Self::Ansi(_) => 1,
        }
    }

    fn decode(&self, raw: &[u8]) -> (String, Result<std::path::PathBuf, BadName>) {
        let text = match self {
            Self::Wide => {
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .take_while(|unit| *unit != 0)
                    .collect();
                String::from_utf16(&units).map_err(|_| String::from_utf16_lossy(&units))
            }
            Self::Ansi(decode) => {
                let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
                decode(&raw[..end]).ok_or_else(|| String::from_utf8_lossy(&raw[..end]).into())
            }
        };
        match text {
            Ok(name) => {
                let path = relative_path(&name);
                (name, path)
            }
            Err(lossy) => (lossy, Err(BadName::NotUnicode)),
        }
    }
}

/// Parses FILEGROUPDESCRIPTOR bytes; trailing bytes after the last entry are ignored.
pub(super) fn parse(bytes: &[u8], names: &Names<'_>) -> Result<Vec<VirtualEntry>, VirtualError> {
    let count = read_u32(bytes, 0).ok_or(VirtualError::Malformed)?;
    let size = FIXED + NAME_UNITS * names.unit();
    let count = usize::try_from(count).map_err(|_| VirtualError::Malformed)?;
    let needed = count
        .checked_mul(size)
        .and_then(|body| body.checked_add(4))
        .ok_or(VirtualError::Malformed)?;
    if bytes.len() < needed {
        return Err(VirtualError::Malformed);
    }
    (0..count)
        .map(|at| {
            let start = 4 + at * size;
            let index = u32::try_from(at).map_err(|_| VirtualError::Malformed)?;
            entry(index, &bytes[start..start + size], names)
        })
        .collect()
}

fn entry(index: u32, raw: &[u8], names: &Names<'_>) -> Result<VirtualEntry, VirtualError> {
    let field = |offset| read_u32(raw, offset).ok_or(VirtualError::Malformed);
    let flags = field(0)?;
    let attributes = field(36)?;
    let (name, path) = names.decode(&raw[FIXED..]);
    let size = (flags & FD_FILESIZE != 0)
        .then(|| Ok::<_, VirtualError>((u64::from(field(64)?) << 32) | u64::from(field(68)?)))
        .transpose()?;
    let written = u64::from(field(56)?) | (u64::from(field(60)?) << 32);
    Ok(VirtualEntry {
        index,
        name,
        path,
        is_dir: flags & FD_ATTRIBUTES != 0 && attributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        size,
        modified: (flags & FD_WRITESTIME != 0)
            .then(|| file_time(written))
            .flatten(),
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(raw.try_into().ok()?))
}

/// FILETIME (100 ns ticks since 1601) as a system time; zero means unknown.
pub(super) fn file_time(ticks: u64) -> Option<SystemTime> {
    if ticks == 0 {
        return None;
    }
    let since_1601 =
        Duration::from_secs(ticks / 10_000_000) + Duration::from_nanos((ticks % 10_000_000) * 100);
    let gap = Duration::from_secs(EPOCH_GAP);
    match since_1601.checked_sub(gap) {
        Some(after) => SystemTime::UNIX_EPOCH.checked_add(after),
        None => SystemTime::UNIX_EPOCH.checked_sub(gap - since_1601),
    }
}

/// The bytes of an HGLOBAL item: exactly the stated size. A block is rounded up, so without a
/// size the file would end in padding. `allocated` is what `GlobalSize` reports.
pub(super) fn global_len(stated: Option<u64>, allocated: usize) -> Result<usize, String> {
    let stated = stated.ok_or("the program did not say how large this file is")?;
    usize::try_from(stated)
        .ok()
        .filter(|len| *len <= allocated)
        .ok_or_else(|| "the program gave less data than the size it announced".to_owned())
}

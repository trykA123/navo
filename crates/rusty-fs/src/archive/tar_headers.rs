use std::io::Read;

use tar::{EntryType, Header, PaxExtensions};

use super::source::{Fail, Source};
use super::{ArchiveEntry, Stamp, damaged};

const BLOCK: usize = 512;
const EXTENSION_LIMIT: u64 = 64 * 1024;
const CHECKSUM: std::ops::Range<usize> = 148..156;
const EXTENDED_SPARSE_FLAG: usize = 504;

pub(super) struct Found {
    pub entry: ArchiveEntry,
    pub stored: u64,
    pub sparse_head: Option<Vec<u8>>,
}

impl Found {
    pub(super) fn padded(&self) -> u64 {
        padded(self.stored)
    }
}

#[derive(Default)]
struct Pending {
    name: Option<Vec<u8>>,
    size: Option<u64>,
}

fn padded(bytes: u64) -> u64 {
    bytes.div_ceil(BLOCK as u64).saturating_mul(BLOCK as u64)
}

fn read_block(source: &mut impl Source) -> Result<Option<[u8; BLOCK]>, Fail> {
    let mut block = [0; BLOCK];
    let mut filled = 0;
    while filled < BLOCK {
        match source.read(&mut block[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(Fail::Damaged(damaged("a tar header is cut short"))),
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(source.fail(error)),
        }
    }
    Ok(Some(block))
}

fn checksum_ok(block: &[u8; BLOCK]) -> bool {
    let sum: u32 = block
        .iter()
        .enumerate()
        .map(|(at, byte)| {
            if CHECKSUM.contains(&at) {
                u32::from(b' ')
            } else {
                u32::from(*byte)
            }
        })
        .sum();
    Header::from_byte_slice(block).cksum().ok() == Some(sum)
}

fn read_extension(source: &mut impl Source, stored: u64) -> Result<Vec<u8>, Fail> {
    if stored > EXTENSION_LIMIT {
        return Err(Fail::Damaged(damaged(
            "a tar header extension is over 64 KB",
        )));
    }
    let mut body = Vec::new();
    source
        .by_ref()
        .take(stored)
        .read_to_end(&mut body)
        .map_err(|error| source.fail(error))?;
    source.skip(padded(stored) - stored)?;
    Ok(body)
}

fn trim_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    bytes.truncate(end);
    bytes
}

fn read_pax(body: &[u8], pending: &mut Pending) {
    for extension in PaxExtensions::new(body).flatten() {
        match extension.key() {
            Ok("path") => pending.name = Some(extension.value_bytes().to_vec()),
            Ok("size") => {
                pending.size = extension.value().ok().and_then(|value| value.parse().ok());
            }
            _ => {}
        }
    }
}

fn sparse_head(source: &mut impl Source, block: &[u8; BLOCK]) -> Result<Vec<u8>, Fail> {
    let mut head = block.to_vec();
    let header = Header::from_byte_slice(block);
    let mut is_extended = header.as_gnu().is_some_and(tar::GnuHeader::is_extended);
    while is_extended {
        let next = read_block(source)?.ok_or_else(|| damaged("a sparse header is cut short"))?;
        is_extended = next[EXTENDED_SPARSE_FLAG] == 1;
        head.extend_from_slice(&next);
    }
    Ok(head)
}

fn describe(header: &Header, pending: Pending, stored: u64) -> ArchiveEntry {
    let modified = header
        .mtime()
        .ok()
        .and_then(|seconds| i64::try_from(seconds).ok())
        .map(|seconds| Stamp::UnixMs(seconds.saturating_mul(1000)));
    let size = if header.entry_type().is_gnu_sparse() {
        header.size().unwrap_or(stored)
    } else {
        stored
    };
    let name = pending
        .name
        .unwrap_or_else(|| header.path_bytes().into_owned());
    ArchiveEntry {
        name: String::from_utf8_lossy(&name).into_owned(),
        size,
        packed: None,
        modified,
        is_folder: header.entry_type().is_dir(),
        is_encrypted: false,
    }
}

pub(super) fn next(source: &mut impl Source) -> Result<Option<Found>, Fail> {
    let mut pending = Pending::default();
    loop {
        let Some(block) = read_block(source)? else {
            return Ok(None);
        };
        if block.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        if !checksum_ok(&block) {
            return Err(Fail::Damaged(damaged(
                "a tar header checksum does not match",
            )));
        }
        let header = Header::from_byte_slice(&block);
        let declared = header.entry_size().map_err(damaged)?;
        match header.entry_type() {
            EntryType::GNULongName => {
                pending.name = Some(trim_nul(read_extension(source, declared)?));
            }
            EntryType::GNULongLink => {
                read_extension(source, declared)?;
            }
            EntryType::XHeader => read_pax(&read_extension(source, declared)?, &mut pending),
            EntryType::XGlobalHeader => source.skip(padded(declared))?,
            kind => {
                let sparse_head = if kind.is_gnu_sparse() {
                    Some(sparse_head(source, &block)?)
                } else {
                    None
                };
                let stored = pending.size.unwrap_or(declared);
                return Ok(Some(Found {
                    entry: describe(header, pending, stored),
                    stored,
                    sparse_head,
                }));
            }
        }
    }
}

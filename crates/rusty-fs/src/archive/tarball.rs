use std::io::{Cursor, Read, Seek};

use flate2::read::MultiGzDecoder;
use lzma_rust2::XzReader;
use ruzstd::decoding::StreamingDecoder;

use super::source::{Fail, Meter, Seekable, Source, Stream};
use super::tar_headers::{Found, next};
use super::{ArchiveFormat, EntryAt, Limits, Listed, damaged, missing, read_capped};

const XZ_MEMORY_KB: u32 = 256 * 1024;

pub(super) fn decompress<'a>(
    format: ArchiveFormat,
    reader: impl Read + 'a,
) -> Result<Box<dyn Read + 'a>, String> {
    Ok(match format {
        ArchiveFormat::TarGz => Box::new(MultiGzDecoder::new(reader)),
        ArchiveFormat::TarXz => Box::new(XzReader::new_mem_limit(reader, true, XZ_MEMORY_KB)),
        ArchiveFormat::TarZst => Box::new(StreamingDecoder::new(reader).map_err(damaged)?),
        _ => Box::new(reader),
    })
}

fn walk(source: &mut impl Source, listed: &mut Listed<'_>) -> Result<(), Fail> {
    while let Some(found) = next(source)? {
        let skip = found.padded();
        if !listed.push(found.entry) {
            break;
        }
        source.skip(skip)?;
    }
    Ok(())
}

fn list_from<'a>(source: &mut impl Source, limits: Limits<'a>) -> Result<Listed<'a>, String> {
    let mut listed = Listed::new(limits);
    match walk(source, &mut listed) {
        Ok(()) => Ok(listed),
        Err(Fail::Stop(_)) => Ok(listed.cut()),
        Err(Fail::Damaged(message)) => Err(message),
    }
}

pub(super) fn list<R: Read + Seek>(
    format: ArchiveFormat,
    reader: R,
    limits: Limits<'_>,
) -> Result<Listed<'_>, String> {
    if format == ArchiveFormat::Tar {
        return list_from(&mut Seekable::new(reader)?, limits);
    }
    let meter = Meter::new(limits.budget, limits.cancel);
    list_from(&mut Stream::new(decompress(format, reader)?, meter), limits)
}

fn read_found(found: Found, source: &mut impl Source, at: EntryAt) -> Result<Vec<u8>, Fail> {
    at.check(&found.entry)?;
    let data = source.by_ref().take(found.stored);
    let read = match found.sparse_head {
        None => read_capped(data, at.limit),
        Some(head) => {
            let mut archive = tar::Archive::new(Cursor::new(head).chain(data));
            let mut entries = archive.entries().map_err(damaged)?;
            let entry = entries.next().ok_or_else(missing)?.map_err(damaged)?;
            read_capped(entry, at.limit)
        }
    };
    read.map_err(|message| source.stopped().map_or(Fail::Damaged(message), Fail::Stop))
}

fn read_from(source: &mut impl Source, at: EntryAt) -> Result<Vec<u8>, Fail> {
    let mut index = 0;
    while let Some(found) = next(source)? {
        if index == at.index {
            return read_found(found, source, at);
        }
        source.skip(found.padded())?;
        index += 1;
    }
    Err(Fail::Damaged(missing()))
}

pub(super) fn read<R: Read + Seek>(
    format: ArchiveFormat,
    reader: R,
    at: EntryAt,
    limits: Limits<'_>,
) -> Result<Vec<u8>, String> {
    let read = if format == ArchiveFormat::Tar {
        read_from(&mut Seekable::new(reader)?, at)
    } else {
        let meter = Meter::new(limits.budget, limits.cancel);
        read_from(&mut Stream::new(decompress(format, reader)?, meter), at)
    };
    read.map_err(Fail::into_read_error)
}

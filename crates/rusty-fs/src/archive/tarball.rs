use std::io::Read;

use flate2::read::MultiGzDecoder;
use lzma_rust2::XzReader;
use ruzstd::decoding::StreamingDecoder;
use tar::EntryType;

use super::{ArchiveEntry, ArchiveFormat, Entry, Stamp, damaged, missing, read_capped};

pub(super) fn decompress<'a>(
    format: ArchiveFormat,
    reader: impl Read + 'a,
) -> Result<Box<dyn Read + 'a>, String> {
    Ok(match format {
        ArchiveFormat::TarGz => Box::new(MultiGzDecoder::new(reader)),
        ArchiveFormat::TarXz => Box::new(XzReader::new(reader, true)),
        ArchiveFormat::TarZst => Box::new(StreamingDecoder::new(reader).map_err(damaged)?),
        _ => Box::new(reader),
    })
}

fn describe<R: Read>(entry: &tar::Entry<'_, R>) -> ArchiveEntry {
    let header = entry.header();
    let modified = header
        .mtime()
        .ok()
        .and_then(|seconds| i64::try_from(seconds).ok())
        .map(|seconds| Stamp::UnixMs(seconds.saturating_mul(1000)));
    ArchiveEntry {
        name: String::from_utf8_lossy(&entry.path_bytes()).into_owned(),
        size: entry.size(),
        packed: None,
        modified,
        is_folder: header.entry_type().is_dir(),
        is_encrypted: false,
    }
}

fn is_listed(kind: EntryType) -> bool {
    !matches!(
        kind,
        EntryType::XGlobalHeader | EntryType::XHeader | EntryType::GNULongName
    )
}

fn each_entry<R: Read>(
    reader: R,
    mut visit: impl FnMut(&mut tar::Entry<'_, R>) -> Result<bool, String>,
) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(damaged)? {
        let mut entry = entry.map_err(damaged)?;
        if is_listed(entry.header().entry_type()) && !visit(&mut entry)? {
            break;
        }
    }
    Ok(())
}

pub(super) fn list(format: ArchiveFormat, reader: impl Read) -> Result<Vec<ArchiveEntry>, String> {
    let mut entries = Vec::new();
    each_entry(decompress(format, reader)?, |entry| {
        entries.push(describe(entry));
        Ok(true)
    })?;
    Ok(entries)
}

pub(super) fn read(format: ArchiveFormat, reader: impl Read, at: Entry) -> Result<Vec<u8>, String> {
    let mut index = 0;
    let mut found = None;
    each_entry(decompress(format, reader)?, |entry| {
        if index < at.index {
            index += 1;
            return Ok(true);
        }
        at.check(&describe(entry))?;
        found = Some(read_capped(entry, at.limit)?);
        Ok(false)
    })?;
    found.ok_or_else(missing)
}

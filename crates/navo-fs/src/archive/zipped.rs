use std::cell::Cell;
use std::io::{self, Read, Seek};

use zip::read::ZipFile;
use zip::result::ZipError;
use zip::{DateTime, ZipArchive};

use super::stream::{StreamRequest, pump};
use super::{
    ArchiveEntry, ArchiveKind, Civil, EntryAt, Limits, Listed, Stamp, UNSUPPORTED_METHOD, damaged,
    missing, read_capped,
};

fn civil(time: DateTime) -> Stamp {
    Stamp::Local(Civil {
        year: time.year(),
        month: time.month(),
        day: time.day(),
        hour: time.hour(),
        minute: time.minute(),
    })
}

fn describe<R: Read>(file: &ZipFile<'_, R>) -> ArchiveEntry {
    let name = file.name().map_or_else(
        |_| String::from_utf8_lossy(file.name_raw()).into_owned(),
        |name| name.into_owned(),
    );
    ArchiveEntry {
        name,
        size: file.size(),
        packed: Some(file.compressed_size()),
        modified: file.last_modified().map(civil),
        kind: if file.is_dir() {
            ArchiveKind::Folder
        } else {
            ArchiveKind::File
        },
        is_encrypted: file.encrypted(),
        header_offset: None,
    }
}

pub(crate) fn open<R: Read + Seek>(reader: R) -> Result<ZipArchive<R>, String> {
    ZipArchive::new(reader).map_err(damaged)
}

fn failed(error: ZipError) -> String {
    match error {
        ZipError::CompressionMethodNotSupported(_) => UNSUPPORTED_METHOD.to_owned(),
        other => damaged(other),
    }
}

/// Reader that notes when zip 9.5's lazy decompressor refuses an unknown method.
struct Probe<'a, R> {
    inner: R,
    refused: &'a Cell<bool>,
}

impl<R: Read> Read for Probe<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer).inspect_err(|error| {
            let method = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<ZipError>());
            if matches!(method, Some(ZipError::CompressionMethodNotSupported(_))) {
                self.refused.set(true);
            }
        })
    }
}

/// Maps a read failure to the "method not supported" text when the probe saw that refusal.
fn refusal<T>(result: Result<T, String>, refused: &Cell<bool>) -> Result<T, String> {
    result.map_err(|error| {
        if refused.get() {
            UNSUPPORTED_METHOD.to_owned()
        } else {
            error
        }
    })
}

pub(super) fn list_archive<'a, R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: Limits<'a>,
) -> Result<Listed<'a>, String> {
    let mut listed = Listed::new(limits);
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index).map_err(damaged)?;
        if !listed.push(describe(&file)) {
            break;
        }
    }
    Ok(listed)
}

pub(super) fn read<R: Read + Seek>(reader: R, at: EntryAt) -> Result<Vec<u8>, String> {
    let mut archive = open(reader)?;
    if at.index >= archive.len() {
        return Err(missing());
    }
    let entry = describe(&archive.by_index_raw(at.index).map_err(damaged)?);
    at.check(&entry)?;
    let file = archive.by_index(at.index).map_err(failed)?;
    let refused = Cell::new(false);
    let probe = Probe {
        inner: file,
        refused: &refused,
    };
    refusal(read_capped(probe, at.limit), &refused)
}

pub(super) fn stream<R: Read + Seek>(reader: R, request: StreamRequest<'_>) -> Result<(), String> {
    let mut archive = open(reader)?;
    stream_archive(&mut archive, request)
}

pub(super) fn stream_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    request: StreamRequest<'_>,
) -> Result<(), String> {
    if request.at.index >= archive.len() {
        return Err(missing());
    }
    let entry = describe(&archive.by_index_raw(request.at.index).map_err(damaged)?);
    request.at.check(&entry)?;
    let file = archive.by_index(request.at.index).map_err(failed)?;
    let refused = Cell::new(false);
    let probe = Probe {
        inner: file,
        refused: &refused,
    };
    refusal(
        pump(probe, request.sender, request.cancel, request.gone).map(drop),
        &refused,
    )
}

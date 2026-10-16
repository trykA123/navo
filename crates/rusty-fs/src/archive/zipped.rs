use std::io::{Read, Seek};

use zip::read::ZipFile;
use zip::result::ZipError;
use zip::{DateTime, ZipArchive};

use super::{
    ArchiveEntry, Civil, EntryAt, Limits, Listed, Stamp, UNSUPPORTED_METHOD, damaged, missing,
    read_capped,
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
        is_folder: file.is_dir(),
        is_encrypted: file.encrypted(),
    }
}

fn open<R: Read + Seek>(reader: R) -> Result<ZipArchive<R>, String> {
    ZipArchive::new(reader).map_err(damaged)
}

fn failed(error: ZipError) -> String {
    match error {
        ZipError::CompressionMethodNotSupported(_) => UNSUPPORTED_METHOD.to_owned(),
        other => damaged(other),
    }
}

pub(super) fn list<R: Read + Seek>(reader: R, limits: Limits<'_>) -> Result<Listed<'_>, String> {
    let mut archive = open(reader)?;
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
    read_capped(file, at.limit)
}

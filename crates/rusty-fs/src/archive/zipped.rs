use std::io::{Read, Seek};

use zip::read::ZipFile;
use zip::{DateTime, ZipArchive};

use super::{ArchiveEntry, Civil, Entry, Stamp, damaged, missing, read_capped};

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

pub(super) fn list<R: Read + Seek>(reader: R) -> Result<Vec<ArchiveEntry>, String> {
    let mut archive = open(reader)?;
    (0..archive.len())
        .map(|index| {
            let file = archive.by_index_raw(index).map_err(damaged)?;
            Ok(describe(&file))
        })
        .collect()
}

pub(super) fn read<R: Read + Seek>(reader: R, at: Entry) -> Result<Vec<u8>, String> {
    let mut archive = open(reader)?;
    if at.index >= archive.len() {
        return Err(missing());
    }
    let entry = describe(&archive.by_index_raw(at.index).map_err(damaged)?);
    at.check(&entry)?;
    let file = archive.by_index(at.index).map_err(damaged)?;
    read_capped(file, at.limit)
}

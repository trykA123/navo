use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use rusty_core::Location;
use rusty_core::encoding::{Encoding, decode, decode_as, encode, looks_binary};
use rusty_core::line_ends::LineEnds;
use rusty_store::replace_atomic;
use rusty_ui::window_view::{FileHooks, FileStamp, FileText, Save, SaveError};

mod archive;
mod code_page;
mod memory;
mod paged;
mod pdf;
mod summary;

const MAX_VIEW: u64 = 16 * 1024 * 1024;
const SNIFF: usize = 8 * 1024;
const MAX_RANGE: usize = 64 * 1024 * 1024;

pub use memory::{SharedDb, open as open_memory};

pub fn hooks(db: Option<SharedDb>) -> FileHooks {
    let hooks = FileHooks {
        read: Arc::new(read),
        read_as: Arc::new(read_as),
        write: Arc::new(write),
        head: Arc::new(paged::head),
        paged: Arc::new(paged::paged),
        paged_as: Arc::new(paged::paged_as),
        archive: Arc::new(archive::archive),
        archive_entry: Arc::new(archive::archive_entry),
        bytes: Arc::new(bytes),
        range: Arc::new(range),
        pdf: Arc::new(pdf::open),
        ..FileHooks::none()
    };
    match db {
        Some(db) => memory::attach(hooks, db),
        None => hooks,
    }
}

fn path_of(location: &Location) -> Result<PathBuf, String> {
    if !location.is_local() {
        return Err("Only local files open here.".to_owned());
    }
    rusty_fs::os_path(location).map_err(|error| error.message())
}

fn stamp_of(path: &std::path::Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified_ms = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())?;
    Some(FileStamp {
        len: metadata.len(),
        modified_ms,
    })
}

pub fn read(location: &Location) -> Result<FileText, String> {
    read_with(location, None)
}

pub fn read_as(location: &Location, encoding: &'static Encoding) -> Result<FileText, String> {
    read_with(location, Some(encoding))
}

fn read_with(location: &Location, chosen: Option<&'static Encoding>) -> Result<FileText, String> {
    let path = path_of(location)?;
    let size = fs::metadata(&path)
        .map_err(|error| format!("Cannot open it: {error}"))?
        .len();
    if size > MAX_VIEW {
        return Err("Too large to open here (over 16 MB). F4 opens it in your editor.".to_owned());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot open it: {error}"))?;
    if chosen.is_none() && looks_binary(&bytes[..bytes.len().min(SNIFF)]) {
        return Err("This looks like a binary file. F4 opens it in your editor.".to_owned());
    }
    let stamp = stamp_of(&path);
    let decoded = match chosen {
        Some(encoding) => decode_as(&bytes, encoding),
        None => decode(&bytes, code_page::ansi()),
    };
    let crlf = mostly_crlf(decoded.text.as_bytes());
    let (text, ends) = match LineEnds::unify(&decoded.text) {
        Some((unified, ends)) => (unified, Some(Arc::new(ends))),
        None => (decoded.text, None),
    };
    Ok(FileText {
        crlf,
        read_only: !decoded.lossless,
        text,
        stamp,
        encoding: decoded.encoding,
        ends,
    })
}

fn too_large(limit: u64) -> String {
    format!(
        "Too large to open here (over {} MB).",
        limit / (1024 * 1024)
    )
}

pub fn bytes(location: &Location, limit: u64) -> Result<Vec<u8>, String> {
    let path = path_of(location)?;
    let file = File::open(&path).map_err(|error| format!("Cannot open it: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Cannot open it: {error}"))?
        .len();
    if size > limit {
        return Err(too_large(limit));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read it: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
}

pub fn range(location: &Location, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    let path = path_of(location)?;
    let mut file = File::open(&path).map_err(|error| format!("Cannot open it: {error}"))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| format!("Cannot read it: {error}"))?;
    let mut bytes = Vec::with_capacity(len.min(MAX_RANGE));
    file.take(len.min(MAX_RANGE) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read it: {error}"))?;
    Ok(bytes)
}

pub fn mostly_crlf(bytes: &[u8]) -> bool {
    let crlf = bytes.windows(2).filter(|pair| *pair == b"\r\n").count();
    let lf = bytes.iter().filter(|byte| **byte == b'\n').count();
    crlf * 2 > lf
}

pub fn line_endings(text: &str, crlf: bool) -> String {
    let unified = text.replace("\r\n", "\n");
    if crlf {
        unified.replace('\n', "\r\n")
    } else {
        unified
    }
}

pub fn write(location: &Location, save: &Save<'_>) -> Result<Option<FileStamp>, SaveError> {
    let path = path_of(location).map_err(SaveError::Failed)?;
    if let Some(expected) = save.expected
        && stamp_of(&path) != Some(expected)
    {
        return Err(SaveError::Changed);
    }
    let text = match save.ends {
        Some(ends) => ends.apply(save.text),
        None => line_endings(save.text, save.crlf),
    };
    let bytes = encode(&text, &save.encoding)
        .map_err(|error| SaveError::Failed(format!("Could not save: {error}")))?;
    replace_atomic(&path, &bytes)
        .map_err(|error| SaveError::Failed(format!("Could not save: {error}")))?;
    Ok(stamp_of(&path))
}

#[cfg(test)]
mod tests;

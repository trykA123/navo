use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use rusty_core::Location;
use rusty_core::encoding::{decode, encode, looks_binary};
use rusty_store::replace_atomic;
use rusty_ui::window_view::{FileHooks, FileStamp, FileText, Save, SaveError};

mod code_page;
mod paged;
mod summary;

const MAX_VIEW: u64 = 16 * 1024 * 1024;
const SNIFF: usize = 8 * 1024;

pub fn hooks() -> FileHooks {
    FileHooks {
        read: Arc::new(read),
        write: Arc::new(write),
        head: Arc::new(paged::head),
        paged: Arc::new(paged::paged),
        bytes: Arc::new(bytes),
        ..FileHooks::none()
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
    let path = path_of(location)?;
    let size = fs::metadata(&path)
        .map_err(|error| format!("Cannot open it: {error}"))?
        .len();
    if size > MAX_VIEW {
        return Err("Too large to open here (over 16 MB). F4 opens it in your editor.".to_owned());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot open it: {error}"))?;
    if looks_binary(&bytes[..bytes.len().min(SNIFF)]) {
        return Err("This looks like a binary file. F4 opens it in your editor.".to_owned());
    }
    let stamp = stamp_of(&path);
    let decoded = decode(&bytes, code_page::ansi());
    let crlf = mostly_crlf(decoded.text.as_bytes());
    Ok(FileText {
        crlf,
        read_only: !decoded.lossless || line_endings(&decoded.text, crlf) != decoded.text,
        text: decoded.text,
        stamp,
        encoding: decoded.encoding,
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
    let text = line_endings(save.text, save.crlf);
    let bytes = encode(&text, &save.encoding)
        .map_err(|error| SaveError::Failed(format!("Could not save: {error}")))?;
    replace_atomic(&path, &bytes)
        .map_err(|error| SaveError::Failed(format!("Could not save: {error}")))?;
    Ok(stamp_of(&path))
}

#[cfg(test)]
mod tests;

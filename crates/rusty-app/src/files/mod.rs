use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::UNIX_EPOCH;

use rusty_core::Location;
use rusty_store::replace_atomic;
use rusty_ui::window_view::{FileHooks, FileStamp, FileText, Save, SaveError};

mod paged;
mod summary;

const MAX_VIEW: u64 = 16 * 1024 * 1024;
const SNIFF: usize = 8 * 1024;

pub fn hooks() -> FileHooks {
    FileHooks {
        read: Rc::new(read),
        write: Rc::new(write),
        pick: Rc::new(paged::pick),
        paged: Rc::new(paged::paged),
    }
}

fn path_of(location: &Location) -> Result<PathBuf, String> {
    if location.is_local() {
        Ok(PathBuf::from(location.as_str()))
    } else {
        Err("Only local files open here.".to_owned())
    }
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
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(
            "UTF-16 text cannot be opened here yet. F4 opens it in your editor.".to_owned(),
        );
    }
    if bytes.iter().take(SNIFF).any(|byte| *byte == 0) {
        return Err("This looks like a binary file. F4 opens it in your editor.".to_owned());
    }
    let crlf = mostly_crlf(&bytes);
    let stamp = stamp_of(&path);
    let (text, read_only) = match String::from_utf8(bytes) {
        Ok(text) => (text, false),
        Err(error) => (String::from_utf8_lossy(error.as_bytes()).into_owned(), true),
    };
    Ok(FileText {
        text,
        read_only,
        crlf,
        stamp,
    })
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
    replace_atomic(&path, text.as_bytes())
        .map_err(|error| SaveError::Failed(format!("Could not save: {error}")))?;
    Ok(stamp_of(&path))
}

#[cfg(test)]
mod tests;

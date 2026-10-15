use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use rusty_core::Location;
use rusty_store::write_atomic;
use rusty_ui::window_view::{FileHooks, FileText};

const MAX_VIEW: u64 = 16 * 1024 * 1024;
const SNIFF: usize = 8 * 1024;

pub fn hooks() -> FileHooks {
    FileHooks {
        read: Rc::new(read),
        write: Rc::new(write),
    }
}

fn path_of(location: &Location) -> Result<PathBuf, String> {
    if location.is_local() {
        Ok(PathBuf::from(location.as_str()))
    } else {
        Err("Only local files open here.".to_owned())
    }
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
    if bytes.iter().take(SNIFF).any(|byte| *byte == 0) {
        return Err("This looks like a binary file. F4 opens it in your editor.".to_owned());
    }
    Ok(match String::from_utf8(bytes) {
        Ok(text) => FileText {
            text,
            read_only: false,
        },
        Err(error) => FileText {
            text: String::from_utf8_lossy(error.as_bytes()).into_owned(),
            read_only: true,
        },
    })
}

pub fn write(location: &Location, text: &str) -> Result<(), String> {
    let path = path_of(location)?;
    let uses_crlf = fs::read(&path).is_ok_and(|old| old.windows(2).any(|pair| pair == b"\r\n"));
    let text = if uses_crlf && !text.contains('\r') {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    };
    write_atomic(&path, text.as_bytes()).map_err(|error| format!("Could not save: {error}"))
}

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{replace_atomic, write_atomic};

mod patch;

pub use patch::{KeymapEdit, patch_keymap};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapFileError {
    #[error("keymap.toml is not valid, so it was left as it is: {0}")]
    FileInvalid(String),
    #[error("the change was not saved: {0}")]
    Rejected(String),
    #[error("keymap.toml keeps its keys in a form Navo cannot edit; use [[bind]] tables")]
    Layout,
    #[error("keymap file error ({kind}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
}

#[derive(Debug)]
pub struct KeymapFile {
    path: PathBuf,
    is_backed_up: bool,
}

impl KeymapFile {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            is_backed_up: false,
        }
    }

    pub fn save<T>(
        &mut self,
        edits: &[KeymapEdit],
        check: impl Fn(&str) -> Result<T, String>,
    ) -> Result<T, KeymapFileError> {
        let text = read(&self.path)?;
        let old = text.as_deref().unwrap_or_default();
        check(old).map_err(KeymapFileError::FileInvalid)?;
        let patched = patch_keymap(old, edits)?;
        let checked = check(&patched).map_err(KeymapFileError::Rejected)?;
        let is_new = text.is_none() && !patched.is_empty();
        if is_new || patched != old {
            self.write(&patched)?;
        }
        Ok(checked)
    }

    fn write(&mut self, text: &str) -> Result<(), KeymapFileError> {
        if self.is_backed_up {
            return replace_atomic(&self.path, text.as_bytes()).map_err(io_error);
        }
        write_atomic(&self.path, text.as_bytes()).map_err(io_error)?;
        self.is_backed_up = true;
        Ok(())
    }
}

fn read(path: &Path) -> Result<Option<String>, KeymapFileError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: io::Error) -> KeymapFileError {
    KeymapFileError::Io {
        kind: error.kind(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;

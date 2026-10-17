use std::io;
use std::path::{Path, PathBuf};

mod patch;

pub use patch::{KeymapEdit, patch_keymap};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapFileError {
    #[error("keymap.toml is not valid, so it was left as it is: {0}")]
    FileInvalid(String),
    #[error("the change was not saved: {0}")]
    Rejected(String),
    #[error("keymap.toml keeps its keys in a form Ferrite cannot edit; use [[bind]] tables")]
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
        let _ = (edits, check, self.is_backed_up, &self.path);
        todo!()
    }
}

#[cfg(test)]
mod tests;

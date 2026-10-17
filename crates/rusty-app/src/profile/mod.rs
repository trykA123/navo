use std::io;

use rusty_store::StoreDirs;
use tempfile::TempDir;

use crate::cli::Start;

pub enum Profile {
    User(Option<StoreDirs>),
    Folder(StoreDirs),
    Temporary { dirs: StoreDirs, _folder: TempDir },
}

impl Profile {
    pub fn dirs(&self) -> Option<&StoreDirs> {
        match self {
            Self::User(dirs) => dirs.as_ref(),
            Self::Folder(dirs) | Self::Temporary { dirs, .. } => Some(dirs),
        }
    }
}

pub fn choose(start: &Start, detect: impl FnOnce() -> Option<StoreDirs>) -> io::Result<Profile> {
    let _ = start;
    Ok(Profile::User(detect()))
}

#[cfg(test)]
mod tests;

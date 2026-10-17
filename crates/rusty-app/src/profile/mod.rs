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
    if let Some(root) = &start.profile {
        return Ok(Profile::Folder(StoreDirs::in_profile(root)));
    }
    if !start.is_metrics() {
        return Ok(Profile::User(detect()));
    }
    let folder = tempfile::Builder::new()
        .prefix("ferrite-metrics-")
        .tempdir()?;
    let dirs = StoreDirs::in_profile(folder.path());
    Ok(Profile::Temporary {
        dirs,
        _folder: folder,
    })
}

#[cfg(test)]
mod tests;

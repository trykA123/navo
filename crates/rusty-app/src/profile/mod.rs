use std::io;
use std::rc::Rc;

use rusty_store::StoreDirs;
use tempfile::TempDir;

use crate::cli::Start;

pub(crate) enum Profile {
    User(Option<StoreDirs>),
    Folder(StoreDirs),
    Temporary { dirs: StoreDirs, cleanup: Cleanup },
}

#[derive(Clone, Default)]
pub(crate) struct Cleanup(Option<Rc<TempDir>>);

impl Cleanup {
    pub(crate) fn on_quit(&self) {
        if let Err(error) = self.run() {
            eprintln!("ferrite: cannot remove the temporary profile: {error}");
        }
    }

    pub(crate) fn run(&self) -> io::Result<()> {
        let Some(folder) = &self.0 else {
            return Ok(());
        };
        match std::fs::remove_dir_all(folder.path()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

impl Profile {
    pub(crate) fn cleanup(&self) -> Cleanup {
        match self {
            Self::Temporary { cleanup, .. } => cleanup.clone(),
            _ => Cleanup::default(),
        }
    }

    pub(crate) fn dirs(&self) -> Option<&StoreDirs> {
        match self {
            Self::User(dirs) => dirs.as_ref(),
            Self::Folder(dirs) | Self::Temporary { dirs, .. } => Some(dirs),
        }
    }
}

pub(crate) fn choose(
    start: &Start,
    detect: impl FnOnce() -> Option<StoreDirs>,
) -> io::Result<Profile> {
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
        cleanup: Cleanup(Some(Rc::new(folder))),
    })
}

#[cfg(test)]
mod tests;

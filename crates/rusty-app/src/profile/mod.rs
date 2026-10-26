use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

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
        let _ = self.run();
    }

    pub(crate) fn run(&self) -> io::Result<()> {
        self.run_with(|path| std::fs::remove_dir_all(path))
    }

    fn run_with(
        &self,
        mut remove: impl FnMut(&std::path::Path) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(folder) = &self.0 else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let error = match remove(folder.path()) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => error,
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(error);
            }
            std::thread::sleep(left.min(Duration::from_millis(25)));
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
    let folder = tempfile::Builder::new().prefix("navo-metrics-").tempdir()?;
    let dirs = StoreDirs::in_profile(folder.path());
    Ok(Profile::Temporary {
        dirs,
        cleanup: Cleanup(Some(Rc::new(folder))),
    })
}

#[cfg(test)]
mod tests;

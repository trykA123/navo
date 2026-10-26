use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::fixture::context;

#[derive(Debug)]
pub(super) struct Workspace {
    root: PathBuf,
    pub(super) fixture: PathBuf,
    pub(super) index: PathBuf,
    is_new_root: bool,
}

impl Workspace {
    pub(super) fn prepare(root: &Path) -> io::Result<Self> {
        let root = std::path::absolute(root).map_err(|error| context("reading", root, error))?;
        let is_new_root = match fs::read_dir(&root).map(|mut entries| entries.next().is_none()) {
            Ok(false) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "{} is not empty; give --root a new or empty folder",
                        root.display()
                    ),
                ));
            }
            Ok(true) => false,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(&root).map_err(|error| context("creating", &root, error))?;
                true
            }
            Err(error) => return Err(context("reading", &root, error)),
        };
        let workspace = Self {
            fixture: root.join("fixture"),
            index: root.join("index"),
            root,
            is_new_root,
        };
        for folder in [&workspace.fixture, &workspace.index] {
            fs::create_dir(folder).map_err(|error| context("creating", folder, error))?;
        }
        Ok(workspace)
    }

    pub(super) fn remove(&self, keep_fixture: bool) -> io::Result<()> {
        let mut doomed = vec![&self.index];
        if !keep_fixture {
            doomed.push(&self.fixture);
        }
        for folder in doomed {
            match fs::remove_dir_all(folder) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(context("removing", folder, error));
                }
                _ => {}
            }
        }
        if self.is_new_root && !keep_fixture {
            let _ = fs::remove_dir(&self.root);
        }
        Ok(())
    }
}

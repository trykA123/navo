use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn open() -> Result<File, String> {
    let directory = directory()?;
    private(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("mutations.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("the shared mutation lock is not a regular file".into());
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => (),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|error| error.to_string())
}

#[cfg(not(test))]
fn directory() -> Result<PathBuf, String> {
    super::anchor::directory()
        .map_err(|error| format!("{}: {error}", rusty_l10n::tr("job-user-unavailable")))
}

#[cfg(test)]
thread_local! {
    static ISOLATED: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct Isolation {
    _root: tempfile::TempDir,
}

#[cfg(test)]
impl Isolation {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        ISOLATED.with(|isolated| *isolated.borrow_mut() = Some(root.path().join("runtime")));
        Self { _root: root }
    }
}

#[cfg(test)]
impl Drop for Isolation {
    fn drop(&mut self) {
        ISOLATED.with(|isolated| *isolated.borrow_mut() = None);
    }
}

#[cfg(test)]
fn directory() -> Result<PathBuf, String> {
    if let Some(path) = ISOLATED.with(|isolated| isolated.borrow().clone()) {
        return Ok(path);
    }
    static ROOT: std::sync::OnceLock<Result<tempfile::TempDir, String>> =
        std::sync::OnceLock::new();
    ROOT.get_or_init(|| tempfile::tempdir().map_err(|error| error.to_string()))
        .as_ref()
        .map(|root| root.path().join("runtime"))
        .map_err(Clone::clone)
}

fn private(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other(
            "mutation lock directory is not a directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != super::anchor::uid() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "mutation lock directory is not private to this user",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_lock_directory_is_isolated_and_shared_within_the_process() {
        let directory = directory().unwrap();
        assert_ne!(directory, super::super::anchor::directory().unwrap());
        assert_eq!(
            std::thread::spawn(|| super::directory().unwrap())
                .join()
                .unwrap(),
            directory
        );
        let _isolation = Isolation::new();
        let first = open().unwrap();
        first.try_lock().unwrap();
        assert!(open().unwrap().try_lock().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn shared_or_symbolic_link_lock_directories_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("runtime");
        private(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private(&path).is_err());
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(private(&alias).is_err());
    }
}

#[cfg(test)]
mod protection_tests {
    use super::*;
    use rusty_fs::ProviderOps;

    #[test]
    fn a_live_lease_cannot_be_unlinked_or_moved_through_navo() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(".navo-mutations-runtime");
        private(&directory).unwrap();
        let lock = directory.join("mutations.lock");
        let lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        lease.lock_shared().unwrap();
        let at = |path: &Path| rusty_core::Location::new(path.to_str().unwrap()).unwrap();
        assert!(rusty_fs::mutation_location(&at(&lock)).is_err());
        assert!(rusty_fs::mutation_location(&at(&directory)).is_err());
        let plan = rusty_core::OperationPlan {
            kind: rusty_core::OpKind::Rename,
            steps: vec![rusty_core::Step::Rename {
                source: at(root.path()),
                target: at(&root.path().with_extension("moved")),
            }],
            totals: rusty_core::Totals {
                files: 0,
                folders: 1,
                bytes: 0,
            },
            conflicts: vec![],
            risks: vec![],
        };
        assert!(
            rusty_fs::check_mutation(
                &plan,
                &rusty_fs::LocalFs::new(),
                &rusty_fs::CancelToken::new()
            )
            .is_err()
        );
        assert!(lock.exists());
        assert!(
            rusty_fs::LocalFs::new()
                .file_identity(&at(&lock))
                .unwrap()
                .is_some()
        );
    }
}

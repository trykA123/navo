use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use rusty_core::brand;

pub(super) fn open(data: &Path) -> Result<File, String> {
    let directory = data.join(format!("{}runtime", brand::MUTATIONS_PREFIX));
    private(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("mutations.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("the profile mutation lock is not a regular file".into());
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

#[cfg(unix)]
fn uid() -> u32 {
    // SAFETY: geteuid takes no arguments and returns the calling process's effective user ID.
    unsafe { libc::geteuid() }
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
        if metadata.uid() != uid() || metadata.permissions().mode() & 0o077 != 0 {
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
    fn the_lock_lives_in_a_reserved_folder_of_the_profile() {
        let root = tempfile::tempdir().unwrap();
        let first = open(root.path()).unwrap();
        first.try_lock().unwrap();
        assert!(open(root.path()).unwrap().try_lock().is_err());
        let folder = fs::read_dir(root.path()).unwrap().next().unwrap().unwrap();
        assert!(brand::is_reserved_name(
            &folder.file_name().to_string_lossy()
        ));
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

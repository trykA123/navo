use std::fs::{self, File};

use rusty_core::{FsError, FsErrorKind, Location};

#[cfg(windows)]
use super::win_copy::{self, CopyProgress};
use super::{LocalFs, error, io_error, os_path};
use crate::{CancelToken, ProviderOps};

#[cfg(any(windows, test))]
fn shell_drops_trailing_dot_or_space(path: &std::path::Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().as_encoded_bytes().last(),
            Some(b'.' | b' ')
        )
    })
}

impl ProviderOps for LocalFs {
    fn copy_file_owned(
        &self,
        request: crate::OwnedCopy<'_>,
        control: crate::CopyControl<'_>,
    ) -> Result<u64, FsError> {
        super::owned::copy(request, control)
    }

    #[cfg(windows)]
    fn remove_owned_file(
        &self,
        location: &Location,
        identity: crate::FileIdentity,
    ) -> Result<(), FsError> {
        super::owned_windows::remove(&os_path(location)?, identity)
            .map_err(|error| io_error(&error, location))
    }

    #[cfg(windows)]
    fn hide_retained(
        &self,
        location: &Location,
        identity: crate::FileIdentity,
    ) -> Result<(), FsError> {
        super::hidden::set(&os_path(location)?, identity)
            .map_err(|error| io_error(&error, location))
    }
    fn available_space(&self, location: &Location) -> Result<Option<u64>, FsError> {
        super::space::available(&os_path(location)?).map_err(|error| io_error(&error, location))
    }

    fn supports_atomic_rename(&self) -> bool {
        cfg!(any(windows, target_os = "linux"))
    }

    fn rename_atomic(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        super::platform::rename_atomic(&os_path(source)?, &os_path(target)?)
            .map_err(|error| io_error(&error, source))
    }

    fn mount_identity(&self, location: &Location) -> Result<Option<u64>, FsError> {
        super::identity::mount(&os_path(location)?).map_err(|error| io_error(&error, location))
    }

    fn file_identity(&self, location: &Location) -> Result<Option<crate::FileIdentity>, FsError> {
        super::identity::read(&os_path(location)?).map_err(|error| io_error(&error, location))
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        fs::create_dir(&path).map_err(|e| io_error(&e, location))
    }

    #[cfg(unix)]
    fn create_private_folder(&self, location: &Location) -> Result<(), FsError> {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(os_path(location)?)
            .map_err(|e| io_error(&e, location))
    }

    #[cfg(unix)]
    fn restrict_to_owner(&self, location: &Location) -> Result<(), FsError> {
        use std::os::unix::fs::PermissionsExt;
        let path = os_path(location)?;
        let found = fs::symlink_metadata(&path).map_err(|e| io_error(&e, location))?;
        if !found.is_dir() {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::NameInvalid)
            });
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|e| io_error(&e, location))
    }

    #[cfg(unix)]
    fn unix_access(&self, location: &Location) -> Result<Option<crate::UnixAccess>, FsError> {
        use std::os::unix::fs::MetadataExt;
        let found = fs::symlink_metadata(os_path(location)?).map_err(|e| io_error(&e, location))?;
        Ok(Some(crate::UnixAccess {
            owner: found.uid(),
            mode: found.mode(),
        }))
    }

    #[cfg(windows)]
    fn owned_by_current_user(&self, location: &Location) -> Result<Option<bool>, FsError> {
        super::win_owner::of_path(&os_path(location)?).map_err(|e| io_error(&e, location))
    }

    #[cfg(windows)]
    fn volume_root(&self, location: &Location) -> Result<Option<Location>, FsError> {
        let mount = super::win_volume::mount_point(&os_path(location)?)
            .map_err(|e| io_error(&e, location))?;
        Ok(
            std::iter::successors(Some(location.clone()), Location::parent).find(|level| {
                os_path(level).is_ok_and(|path| super::win_volume::is_mount_point(&path, &mount))
            }),
        )
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        File::options()
            .write(true)
            .create_new(true)
            .open(os_path(location)?)
            .map(drop)
            .map_err(|e| io_error(&e, location))
    }

    #[cfg(windows)]
    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        win_copy::copy(
            source,
            target,
            &self.auto_tuning,
            CopyProgress::new(progress, cancel),
        )
    }

    #[cfg(not(windows))]
    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        let mut input = File::open(os_path(source)?).map_err(|e| io_error(&e, source))?;
        let metadata = input.metadata().map_err(|e| io_error(&e, source))?;
        let target_path = os_path(target)?;
        let mut output = File::options()
            .write(true)
            .create_new(true)
            .open(&target_path)
            .map_err(|e| io_error(&e, target))?;
        let total = super::copy_pump::pump(
            &mut input,
            &mut output,
            &mut crate::CopyControl { progress, cancel },
        )
        .map_err(|failure| failure.located(source, target))?;
        output.sync_all().map_err(|e| io_error(&e, target))?;
        if let Ok(modified) = metadata.modified() {
            let _ = output.set_modified(modified);
        }
        drop(output);
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            let _ = fs::set_permissions(&target_path, permissions);
        }
        Ok(total)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        let target_path = os_path(target)?;
        if fs::symlink_metadata(&target_path).is_ok_and(|m| m.is_dir()) {
            return Err(error(FsErrorKind::NameInvalid, target));
        }
        super::platform::replace(&os_path(source)?, &target_path).map_err(|e| io_error(&e, target))
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        super::platform::rename_new(&os_path(source)?, &os_path(target)?)
            .map_err(|e| io_error(&e, source))
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        let result = fs::remove_file(&path);
        #[cfg(windows)]
        if let Err(failure) = &result
            && failure.raw_os_error()
                == Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED.cast_signed())
        {
            let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&e, location))?;
            let mut permissions = metadata.permissions();
            if metadata.is_file() && permissions.readonly() {
                #[expect(
                    clippy::permissions_set_readonly_false,
                    reason = "on Windows this clears only the read-only attribute"
                )]
                permissions.set_readonly(false);
                fs::set_permissions(&path, permissions).map_err(|e| io_error(&e, location))?;
                return fs::remove_file(&path).map_err(|e| io_error(&e, location));
            }
        }
        result.map_err(|e| io_error(&e, location))
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&e, location))?;
        if !metadata.file_type().is_symlink() {
            return Err(error(FsErrorKind::NameInvalid, location));
        }
        fs::remove_file(&path)
            .or_else(|_| fs::remove_dir(&path))
            .map_err(|e| io_error(&e, location))
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        fs::remove_dir(os_path(location)?).map_err(|e| io_error(&e, location))
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        #[cfg(windows)]
        if shell_drops_trailing_dot_or_space(&path) {
            return Err(error(FsErrorKind::NameInvalid, location));
        }
        trash::delete(path).map_err(|_| error(FsErrorKind::Unknown, location))
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        super::link::link_at(target_of, at)
    }

    fn link_target(&self, at: &Location) -> Result<Option<Location>, FsError> {
        super::link::link_target(at)
    }

    fn link_points_at(&self, at: &Location, target_of: &Location) -> Result<bool, FsError> {
        super::link::points_at(at, target_of)
    }

    fn volume_serial(&self, location: &Location) -> Result<Option<u64>, FsError> {
        super::platform::volume_serial(&os_path(location)?)
            .map(Some)
            .map_err(|e| io_error(&e, location))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::shell_drops_trailing_dot_or_space;

    #[test]
    fn a_name_ending_in_a_dot_cannot_be_spelled_by_the_shell() {
        assert!(shell_drops_trailing_dot_or_space(Path::new(
            "/tmp/trailingdot."
        )));
    }

    #[test]
    fn a_name_ending_in_a_space_cannot_be_spelled_by_the_shell() {
        assert!(shell_drops_trailing_dot_or_space(Path::new(
            "/tmp/trailing space "
        )));
    }

    #[test]
    fn an_ordinary_name_can_be_spelled_by_the_shell() {
        assert!(!shell_drops_trailing_dot_or_space(Path::new(
            "/tmp/ordinary.txt"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn a_trailing_dot_ancestor_folder_cannot_be_spelled_by_the_shell() {
        assert!(shell_drops_trailing_dot_or_space(Path::new(
            r"\\?\C:\dotted.\ordinary.txt"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn a_trailing_dot_unc_path_cannot_be_spelled_by_the_shell() {
        assert!(shell_drops_trailing_dot_or_space(Path::new(
            r"\\?\UNC\server\share\trailingdot."
        )));
        assert!(!shell_drops_trailing_dot_or_space(Path::new(
            r"\\?\UNC\server\share\ordinary.txt"
        )));
    }
}

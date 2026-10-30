use rusty_core::{FsError, FsErrorKind, Location};

use super::{CancelToken, CopyControl, EntryKind, MoveProof, OwnedCopy, Provider, VerifiedRename};

pub use rusty_core::{FileIdentity, FileIdentityKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnixAccess {
    pub owner: u32,
    pub mode: u32,
}

pub trait ProviderOps: Provider {
    fn hide_retained(&self, location: &Location, identity: FileIdentity) -> Result<(), FsError> {
        if self.matches_identity(location, Some(identity))? {
            return Ok(());
        }
        Err(FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        })
    }
    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        _control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        Err(FsError {
            location: Some(request.target.clone()),
            ..FsError::new(FsErrorKind::NotSupportedYet)
        })
    }
    fn remove_owned_file(
        &self,
        location: &Location,
        identity: FileIdentity,
    ) -> Result<(), FsError> {
        if !self.matches_identity(location, Some(identity))? {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::IdentityUnavailable)
            });
        }
        self.remove_file(location)
    }
    fn available_space(&self, _location: &Location) -> Result<Option<u64>, FsError> {
        Ok(None)
    }
    fn supports_atomic_rename(&self) -> bool {
        false
    }
    fn rename_atomic(&self, source: &Location, _target: &Location) -> Result<(), FsError> {
        Err(FsError {
            location: Some(source.clone()),
            ..FsError::new(FsErrorKind::NotSupportedYet)
        })
    }
    fn mount_identity(&self, _location: &Location) -> Result<Option<u64>, FsError> {
        Ok(None)
    }
    fn file_identity(&self, _location: &Location) -> Result<Option<FileIdentity>, FsError> {
        Ok(None)
    }
    fn matches_identity(
        &self,
        location: &Location,
        expected: Option<FileIdentity>,
    ) -> Result<bool, FsError> {
        let Some(expected) = expected else {
            return Ok(false);
        };
        if let FileIdentityKind::Fingerprint { .. } = expected.kind {
            return match self.metadata(location) {
                Ok(entry) => Ok(FileIdentity::fingerprint(
                    entry.size,
                    entry.modified_ms,
                    entry.kind == EntryKind::Folder,
                )
                .same_fingerprint(&expected)),
                Err(error) if error.kind == FsErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            };
        }
        match self.file_identity(location) {
            Ok(Some(identity)) => Ok(identity == expected),
            Ok(None) => Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::IdentityUnavailable)
            }),
            Err(error) if error.kind == FsErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
    fn create_folder(&self, location: &Location) -> Result<(), FsError>;
    fn create_private_folder(&self, location: &Location) -> Result<(), FsError> {
        self.create_folder(location)
    }
    fn unix_access(&self, _location: &Location) -> Result<Option<UnixAccess>, FsError> {
        Ok(None)
    }
    fn restrict_to_owner(&self, _location: &Location) -> Result<(), FsError> {
        Ok(())
    }
    fn owned_by_current_user(&self, _location: &Location) -> Result<Option<bool>, FsError> {
        Ok(None)
    }
    fn volume_root(&self, _location: &Location) -> Result<Option<Location>, FsError> {
        Ok(None)
    }
    fn create_file(&self, location: &Location) -> Result<(), FsError>;
    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P8 O5 contract"
    )]
    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError>;
    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError>;
    fn rename_verified(
        &self,
        from: &Location,
        to: &Location,
        expect: &MoveProof,
    ) -> Result<VerifiedRename, FsError> {
        super::verified::by_path(self, (from, to), expect)
    }
    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError>;
    fn remove_file(&self, location: &Location) -> Result<(), FsError>;
    fn remove_link(&self, location: &Location) -> Result<(), FsError>;
    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError>;
    fn recycle(&self, location: &Location) -> Result<(), FsError>;
    fn make_link(&self, _target_of: &Location, at: &Location) -> Result<(), FsError> {
        Err(FsError {
            location: Some(at.clone()),
            ..FsError::new(FsErrorKind::NotSupportedYet)
        })
    }
    fn link_target(&self, _at: &Location) -> Result<Option<Location>, FsError> {
        Ok(None)
    }
    fn link_points_at(&self, at: &Location, target_of: &Location) -> Result<bool, FsError> {
        Ok(self.link_target(at)?.as_ref() == Some(target_of))
    }
    fn volume_serial(&self, _location: &Location) -> Result<Option<u64>, FsError> {
        Ok(None)
    }
}

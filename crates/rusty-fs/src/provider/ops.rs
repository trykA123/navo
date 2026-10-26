use rusty_core::{FsError, FsErrorKind, Location};

use super::{CancelToken, EntryKind, FsEntry, Provider};

const DST_SHIFT_MS: u64 = 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIdentityKind {
    Native,
    WindowsIndex,
    Fingerprint { folder: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    pub volume: u64,
    pub kind: FileIdentityKind,
    pub file: [u8; 16],
}

impl FileIdentity {
    #[must_use]
    pub fn fingerprint(entry: &FsEntry) -> Self {
        let mut file = [0; 16];
        file[..8].copy_from_slice(&entry.size.to_le_bytes());
        file[8..].copy_from_slice(&entry.modified_ms.unwrap_or(i64::MIN).to_le_bytes());
        Self {
            volume: 0,
            kind: FileIdentityKind::Fingerprint {
                folder: entry.kind == EntryKind::Folder,
            },
            file,
        }
    }

    #[must_use]
    pub fn same_fingerprint(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        let same_shape = matches!(self.kind, FileIdentityKind::Fingerprint { .. })
            && self.kind == other.kind
            && self.file[..8] == other.file[..8];
        same_shape
            && match (self.modified_ms(), other.modified_ms()) {
                (Some(left), Some(right)) => left.abs_diff(right) == DST_SHIFT_MS,
                _ => false,
            }
    }

    fn modified_ms(&self) -> Option<i64> {
        let mut time = [0; 8];
        time.copy_from_slice(&self.file[8..]);
        Some(i64::from_le_bytes(time)).filter(|modified| *modified != i64::MIN)
    }
}

pub trait ProviderOps: Provider {
    fn supports_atomic_rename(&self) -> bool {
        false
    }
    fn rename_atomic(&self, source: &Location, _target: &Location) -> Result<(), FsError> {
        Err(FsError {
            location: Some(source.clone()),
            ..FsError::new(FsErrorKind::NotSupportedYet)
        })
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
                Ok(entry) => Ok(FileIdentity::fingerprint(&entry).same_fingerprint(&expected)),
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

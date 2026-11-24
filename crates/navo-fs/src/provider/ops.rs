use navo_core::{FsError, FsErrorKind, Location};

use super::{
    CancelToken, CopyControl, EntryKind, MoveProof, OwnedCopy, Provider, StreamWrite,
    VerifiedRename,
};

pub use navo_core::{FileIdentity, FileIdentityKind};

/// How an uncached content hash read its bytes, so a caller can tell a device read from a weaker one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSource {
    /// Read past the system cache (Windows unbuffered handle), or after a cache-drop hint the
    /// system accepted (Linux, a best-effort hint: dirty pages stay cached).
    Device,
    /// Read through the system cache: the provider or volume has no uncached read.
    CacheFallback,
}

/// A BLAKE3 content hash and how its bytes were read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UncachedHash {
    pub hash: [u8; 32],
    pub source: ReadSource,
}

/// Keeps Git reads in a folder paused while it lives; dropping it releases the pause.
#[must_use]
pub struct GitPauseGuard {
    _pause: navo_git::cli::GitPause,
}

impl GitPauseGuard {
    /// Wraps a navo-git pause so callers outside navo-fs see only this opaque guard.
    pub(crate) fn new(pause: navo_git::cli::GitPause) -> Self {
        Self { _pause: pause }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnixAccess {
    pub owner: u32,
    pub mode: u32,
}

pub trait ProviderOps: Provider {
    /// Waits `delay` before a job retries a step or, under a speed cap, sends more bytes; wakes
    /// early on cancel; true when cancelled. A test provider records the delay instead of sleeping.
    fn wait_to_retry(&self, delay: std::time::Duration, cancel: &CancelToken) -> bool {
        cancel.wait_cancelled(delay)
    }

    /// The job engine's clock reading; a test provider moves it only by the waits it recorded.
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

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
    /// Writes a stream into a new file and makes it durable; returns the bytes written.
    fn write_new(
        &self,
        request: StreamWrite<'_>,
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
    /// Stops Git reads under `location` and keeps them out while the guard lives.
    fn pause_git(&self, _location: &Location) -> Result<Option<GitPauseGuard>, FsError> {
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
    /// Makes a file's bytes, or a folder's entries, durable on disk.
    fn flush(&self, _location: &Location) -> Result<(), FsError> {
        Ok(())
    }
    /// BLAKE3 of the file's bytes; stops with `Cancelled` when the token is cancelled.
    fn content_hash(
        &self,
        location: &Location,
        _cancel: &CancelToken,
    ) -> Result<[u8; 32], FsError> {
        Err(FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::NotSupportedYet)
        })
    }
    /// BLAKE3 of the file's bytes read past the system cache where the provider can, with how it
    /// read them; this default hashes through the cache (`CacheFallback`). Stops with `Cancelled`.
    fn content_hash_uncached(
        &self,
        location: &Location,
        cancel: &CancelToken,
    ) -> Result<UncachedHash, FsError> {
        let hash = self.content_hash(location, cancel)?;
        Ok(UncachedHash {
            hash,
            source: ReadSource::CacheFallback,
        })
    }
}

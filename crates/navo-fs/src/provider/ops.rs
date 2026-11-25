use navo_core::{FsError, FsErrorKind, Location};

use super::{
    CancelToken, CopyControl, EntryKind, MoveCopy, MoveProof, OwnedCopy, Provider, StreamWrite,
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

/// A move's source file as its copy read it: the move removes the source only while it still
/// matches. A provider with handles takes it from the handle the copy reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopiedSource {
    /// Volume serial and file ID; `None` where the volume keeps none (FAT, exFAT, some SMB).
    pub identity: Option<FileIdentity>,
    pub size: u64,
    pub modified_ms: Option<i64>,
    /// Creation time and Windows attribute bits stand in for a missing file ID.
    pub created_ms: Option<i64>,
    pub attributes: Option<u32>,
}

impl CopiedSource {
    /// True when this can tell the copied file from a replacement: a file ID, or else modified
    /// time, creation time and attributes together. A move never removes a source without one.
    #[must_use]
    pub fn proves(&self) -> bool {
        self.identity.is_some()
            || (self.modified_ms.is_some()
                && self.created_ms.is_some()
                && self.attributes.is_some())
    }

    /// True when `now` is still this file, unchanged. Without a file ID, `now` must have none
    /// either (one with an ID is on another volume), and a replacement with the same size, times
    /// and attributes passes: the known limit on volumes that keep no IDs. Windows
    /// name tunneling (FAT and NTFS) gives a file created under the same name within about 15 s of
    /// a rename or delete the old creation time, so an editor's safe-save there can pass too.
    #[must_use]
    pub fn still(&self, now: &Self) -> bool {
        let same = self.size == now.size && self.modified_ms == now.modified_ms;
        match self.identity {
            Some(_) => same && now.identity == self.identity,
            None => {
                same && self.proves()
                    && now.identity.is_none()
                    && now.created_ms == self.created_ms
                    && now.attributes == self.attributes
            }
        }
    }

    /// True when the file at `location`, read by path ([`ProviderOps::copied_source_at`]), is
    /// still this one. Where that read has no creation time or attributes, a proof without a file
    /// ID never matches.
    pub(crate) fn still_at<P: ProviderOps + ?Sized>(
        &self,
        ops: &P,
        location: &Location,
    ) -> Result<bool, FsError> {
        Ok(ops
            .copied_source_at(location)?
            .is_some_and(|now| self.still(&now)))
    }
}

/// What `location` is by path: `None` when it is not a plain file. A provider without handles has
/// no creation time or attributes, so without a file ID this proves nothing.
fn seen_by_path<P: ProviderOps + ?Sized>(
    ops: &P,
    location: &Location,
) -> Result<Option<CopiedSource>, FsError> {
    let entry = ops.metadata(location)?;
    if entry.kind != EntryKind::File {
        return Ok(None);
    }
    Ok(Some(CopiedSource {
        identity: ops.file_identity(location)?,
        size: entry.size,
        modified_ms: entry.modified_ms,
        created_ms: None,
        attributes: None,
    }))
}

fn changed(location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(FsErrorKind::ChangedOnDisk)
    }
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
    /// Copies a move's source file and returns the bytes written with the source as the copy read
    /// it; `None` when it changed during the copy or the read could not be checked, and the move
    /// then keeps it as changed on disk. This default, for providers without handles, reads the
    /// source by path before and after the copy: a source swapped away and back during the copy
    /// escapes it. `LocalFs` and `FakeFs` take the proof from the read itself.
    fn copy_moved_file(
        &self,
        request: MoveCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<(u64, Option<CopiedSource>), FsError> {
        let MoveCopy {
            source,
            target,
            created,
        } = request;
        let before = seen_by_path(self, source)?;
        let written = match created {
            Some(created) => self.copy_file_owned(
                OwnedCopy {
                    source,
                    target,
                    created,
                },
                control,
            )?,
            None => self.copy_file(source, target, control.progress, control.cancel)?,
        };
        let after = seen_by_path(self, source).ok().flatten();
        Ok((written, before.filter(|seen| after.as_ref() == Some(seen))))
    }
    /// Removes a moved file's source only while it is still the file that was copied; fails with
    /// `ChangedOnDisk` and keeps it otherwise. This default checks by path, then removes by path,
    /// and `LocalFs` uses it on Linux and macOS: a replacement that lands between the check and
    /// the unlink is deleted (a known limit). Windows checks and deletes through one handle.
    fn remove_copied_source(
        &self,
        location: &Location,
        expected: CopiedSource,
    ) -> Result<(), FsError> {
        if !expected.still_at(self, location)? {
            return Err(changed(location));
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
    /// The file at `location` read by path, in the shape of a move's source proof; `None` when it
    /// is not a plain file or, where a provider says so, could not be read. This default has no
    /// creation time or attributes, so without a file ID it proves nothing.
    fn copied_source_at(&self, location: &Location) -> Result<Option<CopiedSource>, FsError> {
        seen_by_path(self, location)
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
    /// The full path of `location` with links, SUBST drives and mappings resolved; `None` when
    /// the provider cannot resolve one.
    fn canonical(&self, _location: &Location) -> Result<Option<Location>, FsError> {
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

#[cfg(test)]
mod tests {
    use super::CopiedSource;

    /// A source read on a volume without file IDs (FAT, exFAT, some SMB shares).
    fn without_id() -> CopiedSource {
        CopiedSource {
            identity: None,
            size: 10,
            modified_ms: Some(1_000),
            created_ms: Some(500),
            attributes: Some(0x20),
        }
    }

    #[test]
    fn without_a_file_id_an_equal_record_proves_the_source() {
        let read = without_id();
        assert!(read.proves());
        assert!(read.still(&without_id()));
    }

    #[test]
    fn without_a_file_id_a_new_creation_time_or_attributes_is_a_replacement() {
        let read = without_id();
        let created = CopiedSource {
            created_ms: Some(501),
            ..read
        };
        let attributes = CopiedSource {
            attributes: Some(0x21),
            ..read
        };
        assert!(!read.still(&created));
        assert!(!read.still(&attributes));
    }

    #[test]
    fn without_a_file_id_a_missing_creation_time_or_attributes_proves_nothing() {
        let read = without_id();
        for partial in [
            CopiedSource {
                created_ms: None,
                ..read
            },
            CopiedSource {
                attributes: None,
                ..read
            },
            CopiedSource {
                modified_ms: None,
                ..read
            },
        ] {
            assert!(!partial.proves());
            assert!(!partial.still(&partial));
        }
    }

    #[test]
    fn without_a_file_id_a_file_that_has_one_now_is_another_volume() {
        let read = without_id();
        let now = CopiedSource {
            identity: Some(crate::FileIdentity {
                volume: 7,
                file: [9; 16],
                kind: crate::FileIdentityKind::Native,
            }),
            ..read
        };
        assert!(!read.still(&now));
    }
}

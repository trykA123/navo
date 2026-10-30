use rusty_core::{FsError, FsErrorKind, Location};

use super::{FileIdentity, FileIdentityKind, ProviderOps};
use crate::{EntryKind, FileStamp, already_exists};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveProof {
    identity: FileIdentity,
    stamp: Option<FileStamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedRename {
    Renamed,
    Skipped(RenameSkip),
    Unresolved { at: Location },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameSkip {
    Missing,
    Replaced,
    Changed,
    OriginOccupied,
    InUse,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Seen {
    pub(crate) identity: FileIdentity,
    pub(crate) kind: EntryKind,
    pub(crate) size: u64,
    pub(crate) modified_ms: Option<i64>,
}

impl MoveProof {
    #[must_use]
    pub fn durable(identity: FileIdentity, stamp: Option<FileStamp>) -> Option<Self> {
        let durable = matches!(
            identity.kind,
            FileIdentityKind::Native | FileIdentityKind::WindowsIndex
        );
        durable.then_some(Self { identity, stamp })
    }

    #[must_use]
    pub fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub fn stamp(&self) -> Option<FileStamp> {
        self.stamp
    }

    pub(crate) fn refuses(
        &self,
        seen: &Seen,
        at: &Location,
    ) -> Result<Option<RenameSkip>, FsError> {
        if seen.identity != self.identity {
            return Ok(Some(RenameSkip::Replaced));
        }
        let changed = match self.stamp {
            Some(stamp) => {
                seen.kind != EntryKind::File
                    || seen.size != stamp.size
                    || seen.modified_ms != stamp.modified_ms
            }
            None if seen.kind == EntryKind::File => {
                return Err(FsError {
                    location: Some(at.clone()),
                    ..FsError::new(FsErrorKind::IdentityUnavailable)
                });
            }
            None => false,
        };
        Ok(changed.then_some(RenameSkip::Changed))
    }
}

pub(super) fn by_path<P: ProviderOps + ?Sized>(
    ops: &P,
    (from, to): (&Location, &Location),
    expect: &MoveProof,
) -> Result<VerifiedRename, FsError> {
    let entry = match ops.metadata(from) {
        Ok(entry) => entry,
        Err(error) if error.kind == FsErrorKind::NotFound => {
            return Ok(VerifiedRename::Skipped(RenameSkip::Missing));
        }
        Err(error) => return Err(error),
    };
    let Some(identity) = ops.file_identity(from)? else {
        return Err(FsError {
            location: Some(from.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        });
    };
    let seen = Seen {
        identity,
        kind: entry.kind,
        size: entry.size,
        modified_ms: entry.modified_ms,
    };
    if let Some(skip) = expect.refuses(&seen, from)? {
        return Ok(VerifiedRename::Skipped(skip));
    }
    match ops.metadata(to) {
        Ok(_) => return Ok(VerifiedRename::Skipped(RenameSkip::OriginOccupied)),
        Err(error) if error.kind == FsErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match ops.rename(from, to) {
        Ok(()) => Ok(VerifiedRename::Renamed),
        Err(error) if already_exists(&error) => {
            Ok(VerifiedRename::Skipped(RenameSkip::OriginOccupied))
        }
        Err(error) => Err(error),
    }
}

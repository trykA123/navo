use rusty_core::{FsError, FsErrorKind, Location, Step};

use super::aside::presence;
use super::{FileStamp, JobLog, StepOutcome};
use crate::{EntryKind, MoveProof, ProviderOps, RenameSkip, VerifiedRename};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedMove {
    Completed,
    NotStarted,
    Unresolved,
}

pub(super) fn durable_proof(ops: &dyn ProviderOps, source: &Location) -> Option<MoveProof> {
    let entry = ops.metadata(source).ok()?;
    let identity = ops.file_identity(source).ok()??;
    let stamp = (entry.kind == EntryKind::File).then_some(FileStamp {
        size: entry.size,
        modified_ms: entry.modified_ms,
    });
    MoveProof::durable(identity, stamp)
}

pub fn recorded_proof(log: &dyn JobLog, seq: u32) -> Result<Option<MoveProof>, FsError> {
    let Some(identity) = log.rename_identity(seq)? else {
        return Ok(None);
    };
    Ok(MoveProof::durable(identity, log.rename_stamp(seq)?))
}

pub fn undo_move(
    ops: &dyn ProviderOps,
    log: &dyn JobLog,
    (seq, step): (u32, &Step),
) -> Result<StepOutcome, FsError> {
    let Step::Rename { source, target } = step else {
        return Ok(StepOutcome::Skipped);
    };
    let Some(proof) = recorded_proof(log, seq)? else {
        return Err(FsError {
            location: Some(target.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        });
    };
    match ops.rename_verified(target, source, &proof)? {
        VerifiedRename::Renamed => Ok(StepOutcome::Done),
        VerifiedRename::Skipped(RenameSkip::Missing)
            if ops.matches_identity(source, Some(proof.identity()))? =>
        {
            Ok(StepOutcome::Done)
        }
        VerifiedRename::Skipped(RenameSkip::InUse) => Err(FsError {
            location: Some(target.clone()),
            ..FsError::new(FsErrorKind::SharingViolation)
        }),
        VerifiedRename::Skipped(_) => Ok(StepOutcome::Skipped),
        VerifiedRename::Unresolved { at } => Err(FsError {
            location: Some(at),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        }),
    }
}

pub fn settle_prepared_move(
    ops: &dyn ProviderOps,
    step: &Step,
    proof: &MoveProof,
) -> Result<PreparedMove, FsError> {
    let Step::Rename { source, target } = step else {
        return Ok(PreparedMove::Unresolved);
    };
    let expected = Some(proof.identity());
    if ops.matches_identity(source, expected)? {
        return Ok(PreparedMove::NotStarted);
    }
    if ops.matches_identity(target, expected)? && presence(ops, source)?.is_none() {
        return Ok(PreparedMove::Completed);
    }
    Ok(PreparedMove::Unresolved)
}

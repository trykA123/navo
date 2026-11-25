use navo_core::{FsError, FsErrorKind, Location};

use super::aside::Aside;
use super::verify::SourceDigest;
use super::{FileStamp, JobContext, PlacedFile, StepOutcome};
use crate::{
    CancelToken, CopiedSource, CopyControl, FileIdentity, MoveCopy, OwnedCopy, ProviderOps,
};

pub(super) struct Copier<'a> {
    pub ops: &'a dyn ProviderOps,
    pub cancel: &'a CancelToken,
    pub strict: bool,
    pub namespace: Option<&'a str>,
    pub removals: &'a super::remove::Removals<'a>,
    pub sources: Option<&'a super::Sources>,
    pub targets: &'a super::plan_targets::PlanTargets,
    pub verify: Option<&'a super::VerifyCount>,
    pub throttle: Option<&'a super::throttle::Throttle>,
}

pub(super) struct CopyStep<'a> {
    pub seq: u32,
    pub source: &'a Location,
    pub target: &'a Location,
    pub size: u64,
}

pub(super) fn temp_of(target: &Location) -> Result<Location, FsError> {
    navo_core::retention::copy_temporary(target, 0, None).map_err(|_| invalid(target))
}

pub(super) fn legacy_temp_of(target: &Location) -> Result<Location, FsError> {
    let name = target.name().ok_or_else(|| invalid(target))?;
    let parent = target.parent().ok_or_else(|| invalid(target))?;
    let temp = navo_core::ProviderPath::from_text(&format!(
        "{}{}",
        name.to_text(),
        navo_core::brand::LEGACY_TEMP_SUFFIX
    ));
    parent.join(&temp).map_err(|_| invalid(target))
}

pub(super) struct CopyHooks<'a> {
    pub progress: &'a mut dyn FnMut(u64),
    pub created: &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
}

impl Copier<'_> {
    pub(super) fn temporary(&self, step: &CopyStep<'_>) -> Result<Location, FsError> {
        let namespace = if self.strict { self.namespace } else { None };
        navo_core::retention::copy_temporary(step.target, step.seq, namespace)
            .map_err(|_| invalid(step.target))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Copied {
    Skipped,
    /// `source` is a move's source proof, journaled with the placed copy.
    Placed {
        stamp: FileStamp,
        flush: bool,
        source: Option<CopiedSource>,
    },
    Ready {
        stamp: FileStamp,
        flush: bool,
        overwrite: bool,
        temporary: Location,
        identity: Option<FileIdentity>,
        seq: u32,
        source: Option<CopiedSource>,
    },
}

impl Copied {
    pub(super) fn bytes(&self) -> Option<u64> {
        match self {
            Self::Skipped => None,
            Self::Placed { stamp, .. } => Some(stamp.size),
            Self::Ready { stamp, .. } => Some(stamp.size),
        }
    }
}

pub(super) fn copy(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    mut hooks: CopyHooks<'_>,
) -> Result<Copied, FsError> {
    let exists = copier.ops.metadata(step.target).is_ok();
    if exists && !copier.targets.overwrites(step.target) {
        return Ok(Copied::Skipped);
    }
    if exists && !super::stream::crosses(step) {
        refuse_alias(copier.ops, step.source, step.target)?;
    }
    let temp = copier.temporary(step)?;
    if copier.strict {
        if super::aside::presence(copier.ops, &temp)?.is_some() {
            return Err(failure(FsErrorKind::Locked, &temp));
        }
    } else {
        remove_if_present(copier.ops, &temp)?;
    }
    let flush = super::stream::crosses(step);
    let result = write(copier, (step, &temp), &mut hooks).and_then(|(stamp, identity)| {
        let source = copier.removals.proof_of(step.source);
        if exists || copier.strict {
            Ok(Copied::Ready {
                stamp,
                flush,
                overwrite: !copier.strict || copier.targets.overwrites(step.target),
                temporary: temp.clone(),
                identity,
                seq: step.seq,
                source,
            })
        } else {
            copier
                .ops
                .rename(&temp, step.target)
                .map(|()| Copied::Placed {
                    stamp,
                    flush,
                    source,
                })
        }
    });
    if result.is_err() && !copier.strict {
        let _ = remove_if_present(copier.ops, &temp);
    }
    result
}

pub(super) fn place(
    aside: &Aside,
    ctx: &mut JobContext<'_>,
    copied: Copied,
    target: &Location,
) -> Result<StepOutcome, FsError> {
    match copied {
        Copied::Skipped => Ok(StepOutcome::Skipped),
        Copied::Placed {
            stamp,
            flush,
            source,
        } => {
            // Only a non-strict copy lands without an aside, and strict follows retention: a
            // retained job's undo rows stay overwrite proofs, as in the `Ready` arm below.
            debug_assert!(
                !ctx.log.preserve_overwrites(),
                "a retained copy placed directly"
            );
            if !ctx.log.preserve_overwrites() {
                journal(ctx, target, stamp, source)?;
            }
            flush_folder(ctx.ops, flush, target).map(|()| StepOutcome::Done)
        }
        Copied::Ready {
            flush,
            overwrite,
            temporary,
            identity,
            seq,
            stamp,
            source,
        } => {
            let placed = aside.place_over(
                ctx,
                &super::aside::Replacement {
                    source: &temporary,
                    target,
                    allow: overwrite,
                    expected_identity: identity,
                },
            );
            if placed.is_err() && !ctx.log.preserve_overwrites() {
                let _ = temp_of(target).map(|temp| remove_if_present(ctx.ops, &temp));
            }
            placed?;
            // The copy now holds the target; its move source goes only in a later step. A
            // retained job's undo rows are overwrite proofs only (retention completion, also in
            // older builds, rejects any other row), so its rollback keeps the copy instead.
            if source.is_some() && !ctx.log.preserve_overwrites() {
                journal(ctx, target, stamp, source)?;
            }
            if identity.is_some() {
                ctx.log.retire_temporary(seq, ctx.ops)?;
            }
            flush_folder(ctx.ops, flush, target).map(|()| StepOutcome::Done)
        }
    }
}

/// Refuses to write over an existing `target` that may be `source` under another name (a SUBST or
/// mapped drive, a mount point, a link, a hard link), before anything is moved aside. With file
/// IDs on both sides they decide. Without, another volume serial clears the target and the same
/// serial falls back to the resolved paths; known limit: a SUBST drive or second mapping onto
/// one FAT volume that resolving cannot join (FAT has no hard links). Anything unreadable refuses.
pub(super) fn refuse_alias(
    ops: &dyn ProviderOps,
    source: &Location,
    target: &Location,
) -> Result<(), FsError> {
    let unavailable = || failure(FsErrorKind::IdentityUnavailable, target);
    let same = if let (Some(from), Some(found)) =
        (ops.file_identity(source)?, ops.file_identity(target)?)
    {
        (serial(from.volume), from.file) == (serial(found.volume), found.file)
    } else {
        let volume = |at| ops.volume_serial(at).ok().flatten().map(serial);
        let (Some(from), Some(found)) = (volume(source), volume(target)) else {
            return Err(unavailable());
        };
        if from != found {
            return Ok(());
        }
        let real = |at| ops.canonical(at).ok().flatten();
        let (Some(from), Some(found)) = (real(source), real(target)) else {
            return Err(unavailable());
        };
        from.same_place(&found)
    };
    if same {
        return Err(failure(FsErrorKind::SameFile, target));
    }
    Ok(())
}

/// A volume serial cut to 32 bits: `FILE_ID_INFO` reports 64 bits and `dwVolumeSerialNumber` 32,
/// so both forms of one volume match. Extra matches only refuse more.
fn serial(volume: u64) -> u32 {
    volume as u32
}

pub(super) fn journal_placed(
    ctx: &mut JobContext<'_>,
    target: &Location,
    stamp: FileStamp,
) -> Result<(), FsError> {
    journal(ctx, target, stamp, None)
}

/// Journals a placed copy and, for a move, the source proof a crash rollback checks.
fn journal(
    ctx: &mut JobContext<'_>,
    target: &Location,
    stamp: FileStamp,
    source: Option<CopiedSource>,
) -> Result<(), FsError> {
    ctx.log.journal_placed(&PlacedFile {
        target: target.clone(),
        stamp,
        source,
    })
}

fn write(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    hooks: &mut CopyHooks<'_>,
) -> Result<(FileStamp, Option<FileIdentity>), FsError> {
    let Copier { ops, cancel, .. } = *copier;
    let mut identity = None;
    let mut created = |found| {
        (hooks.created)(found)?;
        identity = Some(found);
        Ok(())
    };
    let (written, digest) = transfer(copier, (step, temp), hooks.progress, &mut created)?;
    if cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, step.target));
    }
    let on_disk = ops.metadata(temp)?;
    if (!super::stream::crosses(step) && written != step.size) || on_disk.size != written {
        return Err(failure(FsErrorKind::Unknown, step.target));
    }
    if copier.strict && !ops.matches_identity(temp, identity)? {
        return Err(failure(FsErrorKind::IdentityUnavailable, temp));
    }
    super::verify::check(copier, (step, temp), digest)?;
    Ok((
        FileStamp {
            size: on_disk.size,
            modified_ms: on_disk.modified_ms,
        },
        identity,
    ))
}

/// Copies the bytes of `step` into `temp` under the job's speed cap, if any, and says where the
/// verify check gets the source hash.
fn transfer(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    progress: &mut dyn FnMut(u64),
    created: &mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
) -> Result<(u64, SourceDigest), FsError> {
    let Copier {
        ops,
        cancel,
        strict,
        ..
    } = *copier;
    let mut metered = super::throttle::meter(copier, progress);
    let progress: &mut dyn FnMut(u64) = &mut metered;
    if super::stream::crosses(step) {
        return super::stream::stream(copier, (step, temp), progress, created);
    }
    let control = CopyControl { progress, cancel };
    let written = if copier.removals.moves() {
        let request = MoveCopy {
            source: step.source,
            target: temp,
            created: strict.then_some(created),
        };
        copier.removals.copy_source(ops, request, control)
    } else if strict {
        let request = OwnedCopy {
            source: step.source,
            target: temp,
            created,
        };
        ops.copy_file_owned(request, control)
    } else {
        ops.copy_file(step.source, temp, control.progress, cancel)
    }?;
    Ok((written, SourceDigest::ReadAgain))
}

fn flush_folder(ops: &dyn ProviderOps, flush: bool, target: &Location) -> Result<(), FsError> {
    if !flush {
        return Ok(());
    }
    let folder = target.parent().ok_or_else(|| invalid(target))?;
    ops.flush(&folder)
}

/// The plan scan `PlanTargets::overwrites` replaces; tests keep it as the oracle.
#[cfg(test)]
pub(super) fn overwrites(plan: &navo_core::OperationPlan, target: &Location) -> bool {
    use navo_core::{Resolution, Step};
    plan.conflicts
        .iter()
        .any(|c| c.resolution == Resolution::Overwrite && c.target.same_place(target))
        || plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::WriteFile { target: at, .. } if at.same_place(target)))
}

pub(super) fn remove_if_present(
    ops: &dyn ProviderOps,
    location: &Location,
) -> Result<bool, FsError> {
    if ops.metadata(location).is_err() {
        return Ok(false);
    }
    ops.remove_file(location).map(|()| true)
}

pub(super) fn failure(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

fn invalid(location: &Location) -> FsError {
    failure(FsErrorKind::NameInvalid, location)
}

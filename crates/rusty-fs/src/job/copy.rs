use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Resolution};

use super::aside::Aside;
use super::{FileStamp, JobContext, PlacedFile, StepOutcome};
use crate::{CancelToken, CopyControl, FileIdentity, OwnedCopy, ProviderOps};

pub(super) struct Copier<'a> {
    pub plan: &'a OperationPlan,
    pub ops: &'a dyn ProviderOps,
    pub cancel: &'a CancelToken,
    pub strict: bool,
    pub namespace: Option<&'a str>,
    pub removals: &'a super::remove::Removals<'a>,
}

pub(super) struct CopyStep<'a> {
    pub seq: u32,
    pub source: &'a Location,
    pub target: &'a Location,
    pub size: u64,
}

pub(super) fn temp_of(target: &Location) -> Result<Location, FsError> {
    rusty_core::retention::copy_temporary(target, 0, None).map_err(|_| invalid(target))
}

pub(super) fn legacy_temp_of(target: &Location) -> Result<Location, FsError> {
    let name = target.name().ok_or_else(|| invalid(target))?;
    let parent = target.parent().ok_or_else(|| invalid(target))?;
    let temp = rusty_core::ProviderPath::from_text(&format!(
        "{}{}",
        name.to_text(),
        rusty_core::brand::LEGACY_TEMP_SUFFIX
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
        rusty_core::retention::copy_temporary(step.target, step.seq, namespace)
            .map_err(|_| invalid(step.target))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Copied {
    Skipped,
    Placed(FileStamp),
    Ready {
        overwrite: bool,
        temporary: Location,
        identity: Option<FileIdentity>,
        seq: u32,
    },
}

pub(super) fn copy(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    mut hooks: CopyHooks<'_>,
) -> Result<Copied, FsError> {
    let exists = copier.ops.metadata(step.target).is_ok();
    if exists && !overwrites(copier.plan, step.target) {
        return Ok(Copied::Skipped);
    }
    let temp = copier.temporary(step)?;
    if copier.strict {
        if super::aside::presence(copier.ops, &temp)?.is_some() {
            return Err(failure(FsErrorKind::Locked, &temp));
        }
    } else {
        remove_if_present(copier.ops, &temp)?;
    }
    let result = write(copier, (step, &temp), &mut hooks).and_then(|(stamp, identity)| {
        if exists || copier.strict {
            Ok(Copied::Ready {
                overwrite: !copier.strict || overwrites(copier.plan, step.target),
                temporary: temp.clone(),
                identity,
                seq: step.seq,
            })
        } else {
            copier
                .ops
                .rename(&temp, step.target)
                .map(|()| Copied::Placed(stamp))
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
        Copied::Placed(stamp) => journal_placed(ctx, target, stamp).map(|()| StepOutcome::Done),
        Copied::Ready {
            overwrite,
            temporary,
            identity,
            seq,
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
            if identity.is_some() {
                ctx.log.retire_temporary(seq, ctx.ops)?;
            }
            Ok(StepOutcome::Done)
        }
    }
}

pub(super) fn journal_placed(
    ctx: &mut JobContext<'_>,
    target: &Location,
    stamp: FileStamp,
) -> Result<(), FsError> {
    ctx.log.journal_placed(&PlacedFile {
        target: target.clone(),
        stamp,
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
    let written = if copier.strict {
        ops.copy_file_owned(
            OwnedCopy {
                source: step.source,
                target: temp,
                created: &mut created,
            },
            CopyControl {
                progress: hooks.progress,
                cancel,
            },
        )?
    } else {
        ops.copy_file(step.source, temp, hooks.progress, cancel)?
    };
    if cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, step.target));
    }
    let on_disk = ops.metadata(temp)?;
    if written != step.size || on_disk.size != written {
        return Err(failure(FsErrorKind::Unknown, step.target));
    }
    if copier.strict && !ops.matches_identity(temp, identity)? {
        return Err(failure(FsErrorKind::IdentityUnavailable, temp));
    }
    Ok((
        FileStamp {
            size: on_disk.size,
            modified_ms: on_disk.modified_ms,
        },
        identity,
    ))
}

pub(super) fn overwrites(plan: &OperationPlan, target: &Location) -> bool {
    plan.conflicts
        .iter()
        .any(|c| c.resolution == Resolution::Overwrite && c.target.same_place(target))
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

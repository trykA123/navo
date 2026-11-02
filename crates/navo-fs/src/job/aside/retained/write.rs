use super::super::Aside;
use super::super::write::{check_hold, flush_folder};
use super::{identity, journal, put_back, verify};
use crate::job::copy::failure;
use crate::job::write_file::{Checked, Write};
use crate::{FileStamp, HeldFile, JobContext};
use navo_core::retention::OverwriteProof;
use navo_core::{FsError, FsErrorKind};

impl Aside {
    pub(in crate::job::aside) fn write_retained(
        &self,
        ctx: &mut JobContext<'_>,
        write: &Write<'_>,
        checked: &Checked,
    ) -> Result<(), FsError> {
        let ops = ctx.ops;
        if !ops.supports_atomic_rename() {
            return Err(failure(FsErrorKind::NotSupportedYet, write.target));
        }
        let placed = Some(FileStamp {
            size: checked.staged.size,
            modified_ms: checked.staged.modified_ms,
        });
        let held = HeldFile {
            placed_hash: Some(checked.placed_hash),
            ..self.held_file(ops, &self.on_disk(ops, write.target), placed)?
        };
        let (proof, created) = self.prepare_proof(ops, (&held, write.staged, None))?;
        journal(ctx, (&held, &proof), created)?;
        let container = held
            .held
            .parent()
            .ok_or_else(|| failure(FsErrorKind::NameInvalid, &held.held))?;
        ops.hide_retained(&container, proof.container_identity)?;
        verify(ops, &held, &proof)?;
        ops.rename_atomic(&held.original, &held.held)?;
        let placing = check_hold(ctx, (write, checked), &held).and_then(|()| {
            if identity(ops, write.staged)? != proof.replacement_identity {
                return Err(failure(FsErrorKind::ChangedOnDisk, write.staged));
            }
            ops.rename_atomic(write.staged, write.target)
        });
        if let Err(cause) = placing {
            return Err(put_back_proved(ctx, (&held, &proof), cause));
        }
        flush_folder(ctx, write.target)
    }
}

fn put_back_proved(
    ctx: &mut JobContext<'_>,
    (held, proof): (&HeldFile, &OverwriteProof),
    cause: FsError,
) -> FsError {
    let back = put_back(ctx.ops, held, proof)
        .and_then(|()| flush_folder(ctx, &held.held))
        .and_then(|()| flush_folder(ctx, &held.original));
    match back {
        Ok(()) => cause,
        Err(error) => {
            ctx.log.held_stranded(held);
            error
        }
    }
}

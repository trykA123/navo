use rusty_core::retention::{CopyTemporary, copy_temporary};
use rusty_core::{FsError, FsErrorKind};

use super::JobContext;
use super::copy::{CopyStep, failure};
use crate::EntryKind;

pub(super) fn prepare(
    ctx: &mut JobContext<'_>,
    step: &CopyStep<'_>,
    namespace: Option<&str>,
) -> Result<(), FsError> {
    if !ctx.log.preserve_overwrites() {
        return Ok(());
    }
    let namespace = namespace.ok_or_else(|| failure(FsErrorKind::NotSupportedYet, step.target))?;
    let parent = step
        .target
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, step.target))?;
    if ctx.ops.metadata(&parent)?.kind != EntryKind::Folder {
        return Err(failure(FsErrorKind::IdentityUnavailable, &parent));
    }
    let parent_identity = ctx
        .ops
        .file_identity(&parent)?
        .ok_or_else(|| failure(FsErrorKind::IdentityUnavailable, &parent))?;
    let mount_identity = ctx
        .ops
        .mount_identity(&parent)?
        .ok_or_else(|| failure(FsErrorKind::NotSupportedYet, &parent))?;
    let temporary = copy_temporary(step.target, step.seq, Some(namespace))
        .map_err(|_| failure(FsErrorKind::NameInvalid, step.target))?;
    ctx.log.prepare_temporary(&CopyTemporary {
        temporary_version: 1,
        seq: step.seq,
        target: step.target.clone(),
        temporary,
        parent_identity,
        mount_identity,
        identity: None,
    })
}

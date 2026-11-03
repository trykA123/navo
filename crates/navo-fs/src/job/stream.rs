use std::io::Read;

use navo_core::{FsError, FsErrorKind, Location};

use super::copy::{Copier, CopyStep, failure};
use super::{SourceContent, SourceIdentity};
use crate::{CopyControl, EntryKind, FileIdentity, StreamWrite};

type Created<'a> = &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>;

pub(super) fn crosses(step: &CopyStep<'_>) -> bool {
    step.source.provider() != step.target.provider()
}

pub(super) fn stream(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    progress: &mut dyn FnMut(u64),
    created: Created<'_>,
) -> Result<u64, FsError> {
    let reader = copier
        .sources
        .and_then(|sources| sources.find(step.source.provider()))
        .ok_or_else(|| failure(FsErrorKind::NotSupportedYet, step.source))?;
    let entry = reader.metadata(step.source)?;
    if entry.kind != EntryKind::File {
        return Err(failure(FsErrorKind::NotSupportedYet, step.source));
    }
    let limit = match entry.identity {
        SourceIdentity::Stamp(stamp) if stamp.size != step.size => {
            return Err(failure(FsErrorKind::ChangedOnDisk, step.source));
        }
        SourceIdentity::Stamp(stamp) => stamp.size.saturating_add(1),
        SourceIdentity::Object(_) | SourceIdentity::Advisory => u64::MAX,
    };
    let mut announced = false;
    let content = {
        let mut announce = || announce_temp(copier, temp, &mut announced, created);
        reader.open_into(step.source, (temp, &mut announce), copier.cancel)?
    };
    let written = match content {
        SourceContent::Stream(bytes) => copier.ops.write_new(
            StreamWrite {
                bytes: &mut bytes.take(limit),
                source: step.source,
                target: temp,
                created: if copier.strict { Some(created) } else { None },
                modified: entry.modified,
            },
            CopyControl {
                progress,
                cancel: copier.cancel,
            },
        )?,
        SourceContent::Written => adopt(copier, temp, progress, (announced, created))?,
    };
    let after = reader.metadata(step.source)?;
    if after != entry
        || matches!(entry.identity, SourceIdentity::Stamp(stamp) if written != stamp.size)
    {
        return Err(failure(FsErrorKind::ChangedOnDisk, step.source));
    }
    Ok(written)
}

/// A file the source wrote itself: the job only learns its size and, when strict, its identity
/// if the source did not announce the file earlier.
fn adopt(
    copier: &Copier<'_>,
    temp: &Location,
    progress: &mut dyn FnMut(u64),
    (mut announced, created): (bool, Created<'_>),
) -> Result<u64, FsError> {
    announce_temp(copier, temp, &mut announced, created)?;
    let size = copier.ops.metadata(temp)?.size;
    progress(size);
    Ok(size)
}

/// Records the source's temporary file once when the job is strict.
fn announce_temp(
    copier: &Copier<'_>,
    temp: &Location,
    announced: &mut bool,
    created: Created<'_>,
) -> Result<(), FsError> {
    if !copier.strict || *announced {
        return Ok(());
    }
    let identity = copier
        .ops
        .file_identity(temp)?
        .ok_or_else(|| failure(FsErrorKind::IdentityUnavailable, temp))?;
    created(identity)?;
    *announced = true;
    Ok(())
}

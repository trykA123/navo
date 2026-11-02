use std::io::Read;

use navo_core::{FsError, FsErrorKind, Location};

use super::SourceIdentity;
use super::copy::{Copier, CopyStep, failure};
use crate::{CopyControl, EntryKind, FileIdentity, StreamWrite};

pub(super) fn crosses(step: &CopyStep<'_>) -> bool {
    step.source.provider() != step.target.provider()
}

pub(super) fn stream(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    progress: &mut dyn FnMut(u64),
    created: &mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
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
        SourceIdentity::Object(_) => u64::MAX,
    };
    let mut bytes = reader.open_read(step.source)?.take(limit);
    let written = copier.ops.write_new(
        StreamWrite {
            bytes: &mut bytes,
            source: step.source,
            target: temp,
            created: if copier.strict { Some(created) } else { None },
        },
        CopyControl {
            progress,
            cancel: copier.cancel,
        },
    )?;
    let after = reader.metadata(step.source)?;
    if after != entry
        || matches!(entry.identity, SourceIdentity::Stamp(stamp) if written != stamp.size)
    {
        return Err(failure(FsErrorKind::ChangedOnDisk, step.source));
    }
    Ok(written)
}

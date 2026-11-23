use std::io::Read;
use std::time::SystemTime;

use navo_core::{FsError, FsErrorKind, Location};

use super::copy::{Copier, CopyStep, failure};
use super::verify::{HashingRead, SourceDigest};
use super::{SourceContent, SourceIdentity};
use crate::{CopyControl, EntryKind, FileIdentity, StreamWrite};

type Created<'a> = &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>;

pub(super) fn crosses(step: &CopyStep<'_>) -> bool {
    step.source.provider() != step.target.provider()
}

/// Copies a step from another provider into `temp`; returns the bytes written and the streamed
/// digest when the job verifies. Fails with `ChangedOnDisk` when the source changed meanwhile.
pub(super) fn stream(
    copier: &Copier<'_>,
    (step, temp): (&CopyStep<'_>, &Location),
    progress: &mut dyn FnMut(u64),
    created: Created<'_>,
) -> Result<(u64, SourceDigest), FsError> {
    let reader = copier
        .sources
        .and_then(|sources| sources.find(step.source.provider()))
        .ok_or_else(|| failure(FsErrorKind::NotSupportedYet, step.source))?;
    let entry = reader.metadata_with_cancel(step.source, copier.cancel)?;
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
    let (written, digest) = match content {
        SourceContent::Stream(bytes) => write_stream(
            copier,
            (step, temp, entry.modified),
            &mut bytes.take(limit),
            (progress, created),
        )?,
        SourceContent::Written => (
            adopt(copier, temp, progress, (announced, created))?,
            SourceDigest::Unavailable,
        ),
    };
    let after = reader.metadata_with_cancel(step.source, copier.cancel)?;
    if after != entry
        || matches!(entry.identity, SourceIdentity::Stamp(stamp) if written != stamp.size)
    {
        return Err(failure(FsErrorKind::ChangedOnDisk, step.source));
    }
    Ok((written, digest))
}

/// Writes `bytes` into the new file `temp` through the target provider. When the job verifies,
/// the bytes are hashed (BLAKE3) as the provider reads them: the source may not be readable twice.
fn write_stream(
    copier: &Copier<'_>,
    (step, temp, modified): (&CopyStep<'_>, &Location, Option<SystemTime>),
    bytes: &mut dyn Read,
    (progress, created): (&mut dyn FnMut(u64), Created<'_>),
) -> Result<(u64, SourceDigest), FsError> {
    let mut write = |bytes: &mut dyn Read| {
        let request = StreamWrite {
            bytes,
            source: step.source,
            target: temp,
            created: if copier.strict {
                Some(&mut *created)
            } else {
                None
            },
            modified,
        };
        let control = CopyControl {
            progress: &mut *progress,
            cancel: copier.cancel,
        };
        copier.ops.write_new(request, control)
    };
    if copier.verify.is_none() {
        return Ok((write(bytes)?, SourceDigest::Unavailable));
    }
    let mut hashing = HashingRead::new(bytes);
    let written = write(&mut hashing)?;
    Ok((written, SourceDigest::Streamed(hashing.digest())))
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

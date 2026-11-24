use std::fs::{File, OpenOptions};

use navo_core::{FsError, FsErrorKind};

use super::super::{copied_source, copy_pump, error, identity, io_error, os_path};
use crate::{CopiedSource, CopyControl, MoveCopy, OwnedCopy};

pub(super) fn copy(request: &mut OwnedCopy<'_>, control: CopyControl<'_>) -> Result<u64, FsError> {
    let request = MoveCopy {
        source: request.source,
        target: request.target,
        created: Some(&mut *request.created),
    };
    copy_moved(request, control).map(|(written, _)| written)
}

/// Copies `request.source` into the new file `request.target`; returns the bytes written and the
/// source as read through the handle the bytes came from.
pub(super) fn copy_moved(
    request: MoveCopy<'_>,
    mut control: CopyControl<'_>,
) -> Result<(u64, CopiedSource), FsError> {
    if control.cancel.is_cancelled() {
        return Err(error(FsErrorKind::Cancelled, request.target));
    }
    let mut input = File::open(os_path(request.source)?)
        .map_err(|failure| io_error(&failure, request.source))?;
    let metadata = input
        .metadata()
        .map_err(|failure| io_error(&failure, request.source))?;
    let read =
        copied_source(&input, &metadata).map_err(|failure| io_error(&failure, request.source))?;
    let mut output =
        open(&os_path(request.target)?).map_err(|failure| io_error(&failure, request.target))?;
    if let Some(created) = request.created {
        let identity = identity::from_file(&output)
            .map_err(|failure| io_error(&failure, request.target))?
            .ok_or_else(|| error(FsErrorKind::IdentityUnavailable, request.target))?;
        created(identity)?;
    }
    let total = copy_pump::pump(&mut input, &mut output, &mut control)
        .map_err(|failure| failure.located(request.source, request.target))?;
    finish(&output, &metadata).map_err(|failure| io_error(&failure, request.target))?;
    Ok((total, read))
}

fn open(path: &std::path::Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    options.open(path)
}

fn finish(output: &File, metadata: &std::fs::Metadata) -> std::io::Result<()> {
    if let Ok(modified) = metadata.modified() {
        output.set_modified(modified)?;
    }
    if metadata.permissions().readonly() {
        output.set_permissions(metadata.permissions())?;
    }
    output.sync_all()
}

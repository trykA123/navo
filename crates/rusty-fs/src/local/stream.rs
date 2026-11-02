use std::fs::OpenOptions;

use rusty_core::{FsError, FsErrorKind};

use super::{copy_pump, error, identity, io_error, os_path};
use crate::{CopyControl, StreamWrite};

pub(super) fn write_new(
    request: StreamWrite<'_>,
    mut control: CopyControl<'_>,
) -> Result<u64, FsError> {
    let StreamWrite {
        bytes,
        source,
        target,
        created,
    } = request;
    if control.cancel.is_cancelled() {
        return Err(error(FsErrorKind::Cancelled, target));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(os_path(target)?)
        .map_err(|failure| io_error(&failure, target))?;
    if let Some(created) = created {
        let identity = identity::from_file(&output)
            .map_err(|failure| io_error(&failure, target))?
            .ok_or_else(|| error(FsErrorKind::IdentityUnavailable, target))?;
        created(identity)?;
    }
    let total = copy_pump::pump(bytes, &mut output, &mut control)
        .map_err(|failure| failure.located(source, target))?;
    output
        .sync_all()
        .map_err(|failure| io_error(&failure, target))?;
    Ok(total)
}

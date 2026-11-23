//! Cloud guard for quick view: a cloud placeholder (a OneDrive stub) is never read automatically,
//! so moving the cursor never downloads it. F3 and Enter open the file as before.

use navo_core::{FsErrorKind, Location};
use navo_fs::{FsEntry, Provider};

/// Says whether quick view may read `location`. The live row's cloud bit stops it at once;
/// otherwise a fresh stat decides, because a row from the disk cache always reads false.
/// Fails with the text to show: the cloud message, or the stat error (a failed check stops the read).
pub(in crate::window_view) fn may_read(
    entry: &FsEntry,
    provider: &dyn Provider,
    location: &Location,
) -> Result<(), String> {
    if entry.attributes.cloud_placeholder {
        return Err(navo_l10n::tr("preview-cloud-file"));
    }
    match provider.metadata(location) {
        Ok(fresh) if fresh.attributes.cloud_placeholder => Err(navo_l10n::tr("preview-cloud-file")),
        Ok(_) => Ok(()),
        // Nothing is there to download; the read reports the missing file itself.
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.message()),
    }
}

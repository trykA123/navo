use crate::{CopyControl, OwnedCopy};
use navo_core::FsError;

#[cfg(not(windows))]
mod buffered;

pub(super) fn copy(request: OwnedCopy<'_>, control: CopyControl<'_>) -> Result<u64, FsError> {
    #[cfg(windows)]
    {
        super::win_copy::copy_owned(request, control)
    }
    #[cfg(not(windows))]
    {
        let mut request = request;
        buffered::copy(&mut request, control)
    }
}

/// Copies a move's source on Linux and macOS; Windows has its own in `win_copy`.
#[cfg(not(windows))]
pub(super) fn copy_moved(
    request: crate::MoveCopy<'_>,
    control: CopyControl<'_>,
) -> Result<(u64, crate::CopiedSource), FsError> {
    buffered::copy_moved(request, control)
}

#[cfg(test)]
mod tests;

use crate::{CopyControl, OwnedCopy};
use rusty_core::FsError;

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

#[cfg(test)]
mod tests;

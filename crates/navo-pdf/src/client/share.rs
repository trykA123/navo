//! Parent side of the handle hand-off: Navo opens the PDF itself, checks its size and shares a
//! read-only duplicate into the helper, so the helper never receives a path (S1b).

use std::fs::{File, OpenOptions};
use std::io;
#[cfg(windows)]
use std::os::windows::io::{AsHandle, BorrowedHandle};
use std::sync::Arc;

use super::Launcher;
use super::helper::Helper;
use crate::pdfium::MAX_FILE_BYTES;
use crate::protocol::PdfError;
use crate::serve::clip;

/// Duplicates the parent's `File` into the helper process (its handle) and returns the handle
/// value valid inside the helper. `navo-app` plugs `navo_shell::share_read_handle` in here.
#[cfg(windows)]
pub type ShareFile = Arc<dyn Fn(&File, BorrowedHandle<'_>) -> io::Result<u64> + Send + Sync>;

/// Test-only stand-in for `ShareFile` where no process handle exists; returns the value to send.
#[cfg(not(windows))]
#[doc(hidden)]
pub type TestShare = Arc<dyn Fn(&File) -> io::Result<u64> + Send + Sync>;

#[cfg(windows)]
pub(super) type Share = ShareFile;
#[cfg(not(windows))]
pub(super) type Share = TestShare;

impl Launcher {
    /// Makes every Open share a read-only handle into the helper instead of sending the path.
    #[cfg(windows)]
    #[must_use]
    pub fn with_share(mut self, share: ShareFile) -> Self {
        self.share = Some(share);
        self
    }

    /// Test-only: the Open order of `with_share` on platforms without process handles.
    #[cfg(not(windows))]
    #[doc(hidden)]
    #[must_use]
    pub fn with_test_share(mut self, share: TestShare) -> Self {
        self.share = Some(share);
        self
    }
}

/// Opens `path` read-only for sharing and checks its size before any helper is touched.
/// Fails with `TooLarge` above `MAX_FILE_BYTES`, or `Unreadable` when it cannot be opened or sized.
pub(super) fn open_checked(path: &str) -> Result<File, PdfError> {
    let unreadable = |error: io::Error| PdfError::Unreadable(clip(error.to_string()));
    let mut options = OpenOptions::new();
    options.read(true);
    // FILE_SHARE_READ | WRITE | DELETE: viewing a PDF never blocks another program saving it.
    #[cfg(windows)]
    std::os::windows::fs::OpenOptionsExt::share_mode(&mut options, 0x1 | 0x2 | 0x4);
    let file = options.open(path).map_err(unreadable)?;
    if file.metadata().map_err(unreadable)?.len() > MAX_FILE_BYTES {
        return Err(PdfError::TooLarge);
    }
    Ok(file)
}

/// Shares `file` into `helper` and returns the value for `OpenSource::Handle`.
/// Fails with `HelperDied` when the helper has no process or the hook fails; the caller drops it.
pub(super) fn share_into(share: &Share, file: &File, helper: &Helper) -> Result<u64, PdfError> {
    #[cfg(windows)]
    let shared = helper
        .child
        .as_ref()
        .map(|child| share(file, child.as_handle()));
    #[cfg(not(windows))]
    let shared = helper.child.as_ref().map(|_| share(file));
    match shared {
        Some(Ok(value)) => Ok(value),
        Some(Err(_)) | None => Err(PdfError::HelperDied),
    }
}

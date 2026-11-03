//! Virtual files: FileGroupDescriptor plus FileContents, offered by Remote Desktop, Outlook,
//! Explorer zip folders and phones instead of real paths (D104).

#[cfg(any(windows, test))]
mod descriptor;
#[cfg(any(windows, test))]
mod hints;
mod names;
#[cfg(windows)]
mod ole;
mod reader;
mod serve;

#[cfg(not(windows))]
mod ole {
    pub(super) fn read_clipboard() -> Result<Option<super::VirtualClipboard>, super::VirtualError> {
        Ok(None)
    }

    pub(super) fn offered() -> bool {
        false
    }
}

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

pub use names::BadName;
#[cfg(windows)]
pub use ole::VirtualSource;
pub use reader::{LONG_STALL, STALL, VirtualClipboard, VirtualContent, VirtualReader};
pub use serve::{Backend, ItemSource, Opened};

/// Returns true once the job is cancelled; polled between short waits.
pub type Cancel = Arc<dyn Fn() -> bool + Send + Sync>;

/// One descriptor entry. `index` is the FileContents `lindex`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualEntry {
    pub index: u32,
    /// The name as offered, for messages.
    pub name: String,
    /// The validated path below the target folder.
    pub path: Result<PathBuf, BadName>,
    pub is_dir: bool,
    /// `None` when the source does not give one.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
}

/// Where the files come from, when that is cheap to know. Only for status text and
/// Mark-of-the-Web decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    RemoteSession,
    Mail,
    Archive,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VirtualError {
    #[error("The file list on the clipboard is damaged")]
    Malformed,
    #[error("Cancelled")]
    Cancelled,
    #[error("The remote session stopped sending data")]
    Stalled,
    #[error("{0}")]
    Source(String),
}

impl VirtualError {
    /// The virtual-file error inside an I/O error from [`VirtualReader`].
    #[must_use]
    pub fn of(error: &std::io::Error) -> Option<&Self> {
        error.get_ref()?.downcast_ref()
    }
}

/// Reads the virtual files on the clipboard. `Ok(None)` when it offers none. Blocks until the
/// file list arrives (at most [`STALL`]), so never call it on the UI thread.
pub fn read_virtual_files() -> Result<Option<VirtualClipboard>, VirtualError> {
    ole::read_clipboard()
}

/// Reads the virtual files of a drop or any other data object. Blocks until the file list
/// arrives, so call it off the thread that owns the object; that thread must keep pumping
/// messages.
#[cfg(windows)]
pub fn read_virtual_files_from(
    source: VirtualSource,
) -> Result<Option<VirtualClipboard>, VirtualError> {
    ole::read_object(source)
}

/// Never blocks: only asks whether a file descriptor is offered.
#[must_use]
pub fn has_virtual_files() -> bool {
    ole::offered()
}

#[cfg(test)]
mod tests;

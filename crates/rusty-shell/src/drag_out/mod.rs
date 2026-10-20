#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

use std::path::{Path, PathBuf};

use crate::shell_menu::folder_key;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DragError {
    #[error("no items to drag")]
    Empty,
    #[error("the dragged items are in different folders")]
    MixedFolders,
    #[error("cannot resolve {path}: {message}")]
    Path { path: PathBuf, message: String },
    #[error("{context}: {message}")]
    Shell {
        context: &'static str,
        message: String,
    },
    #[error("dragging files out is not supported on this platform")]
    Unsupported,
}

/// What the drop target reported. `NoEffect` covers a cancel, a refusal and an optimized move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOutcome {
    Copied,
    Moved,
    Linked,
    NoEffect,
}

/// Hands `paths`, which must share one parent folder, to the system as a file drag.
///
/// Call it on the UI thread that owns `hwnd`, from a message posted while the left button is
/// down. The call is a nested modal loop that pumps that thread's messages until the drop, so
/// no `RefCell` borrow, lock or `&mut` app state may be held across it; re-read state after it.
pub fn drag_files(hwnd: isize, paths: &[PathBuf]) -> Result<DropOutcome, DragError> {
    same_folder(paths)?;
    platform::drag_files(hwnd, paths)
}

/// The shell data object for `paths`, which must share one parent folder.
#[cfg(windows)]
pub fn data_object(
    paths: &[PathBuf],
) -> Result<windows::Win32::System::Com::IDataObject, DragError> {
    same_folder(paths)?;
    platform::data_object(paths)
}

fn same_folder(paths: &[PathBuf]) -> Result<(), DragError> {
    let (first, rest) = paths.split_first().ok_or(DragError::Empty)?;
    let key = |path: &Path| {
        folder_key(path).map_err(|error| DragError::Path {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    };
    let parent = key(first)?;
    for path in rest {
        if key(path)? != parent {
            return Err(DragError::MixedFolders);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

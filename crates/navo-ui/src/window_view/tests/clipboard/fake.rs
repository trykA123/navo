//! The fake file clipboard for window tests: it holds what Navo wrote and refuses what the real
//! Windows clipboard refuses, so a location without an OS path fails here too.

use crate::browser::StreamPaste;
use crate::window_view::{ClipboardFault, ClipboardFiles, FileClipboardService};

/// Fails like the app's `path_of` in `navo-app/src/clipboard.rs`: only a location with an OS
/// path (never one inside an archive) can go on the system clipboard. `fake://` locations stand
/// in for the disk in window tests, so they pass.
fn os_path(item: &navo_core::Location) -> Result<(), ClipboardFault> {
    if item.provider().as_str() == "fake" {
        return Ok(());
    }
    navo_fs::os_path(item)
        .map(drop)
        .map_err(|error| ClipboardFault::Failed(error.message()))
}

/// A test clipboard: `writes` and `clears` record every call, `held` is what a read returns.
#[derive(Default)]
pub(in crate::window_view) struct FakeClipboard {
    pub held: std::sync::Mutex<Option<ClipboardFiles>>,
    pub writes: std::sync::Mutex<Vec<ClipboardFiles>>,
    pub clears: std::sync::Mutex<Vec<ClipboardFiles>>,
    pub unsupported: bool,
    pub virtual_files: std::sync::Mutex<Option<StreamPaste>>,
}

impl FileClipboardService for FakeClipboard {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        files.items.iter().try_for_each(os_path)?;
        self.writes.lock().unwrap().push(files.clone());
        *self.held.lock().unwrap() = Some(files.clone());
        Ok(())
    }

    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        Ok(self.held.lock().unwrap().clone())
    }

    fn has_files(&self) -> bool {
        let held = self.held.lock().unwrap();
        !self.unsupported && held.as_ref().is_some_and(|files| !files.items.is_empty())
    }

    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault> {
        if self.unsupported {
            return Err(ClipboardFault::Unsupported);
        }
        files.items.iter().try_for_each(os_path)?;
        self.clears.lock().unwrap().push(files.clone());
        let mut held = self.held.lock().unwrap();
        let is_same = held.as_ref() == Some(files);
        if is_same {
            *held = None;
        }
        Ok(is_same)
    }

    fn read_virtual(&self) -> Result<Option<StreamPaste>, ClipboardFault> {
        Ok(self.virtual_files.lock().unwrap().take())
    }

    fn has_virtual_files(&self) -> bool {
        self.virtual_files.lock().unwrap().is_some()
    }
}

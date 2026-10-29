use std::path::PathBuf;
use std::sync::Arc;

use rusty_core::Location;
use rusty_shell::{ClipboardError, Effect, FileClipboard};
use rusty_ui::window_view::{
    ClipboardEffect, ClipboardFault, ClipboardFiles, FileClipboardService,
};

use crate::shell_menu::without_verbatim;
use crate::startup::path_location;

pub fn service() -> Arc<dyn FileClipboardService> {
    Arc::new(ShellClipboard)
}

struct ShellClipboard;

fn fault(error: ClipboardError) -> ClipboardFault {
    match error {
        ClipboardError::Unsupported => ClipboardFault::Unsupported,
        other => ClipboardFault::Failed(other.to_string()),
    }
}

fn path_of(item: &Location) -> Result<PathBuf, ClipboardFault> {
    rusty_fs::os_path(item)
        .map(|path| PathBuf::from(without_verbatim(path.into_os_string())))
        .map_err(|error| ClipboardFault::Failed(error.message()))
}

fn to_shell(files: &ClipboardFiles) -> Result<FileClipboard, ClipboardFault> {
    let paths = files.items.iter().map(path_of).collect::<Result<_, _>>()?;
    let effect = match files.effect {
        ClipboardEffect::Copy => Effect::Copy,
        ClipboardEffect::Move => Effect::Move,
    };
    Ok(FileClipboard { paths, effect })
}

fn from_shell(files: &FileClipboard) -> Result<ClipboardFiles, ClipboardFault> {
    let items = files
        .paths
        .iter()
        .map(|path| {
            path_location(path).ok_or_else(|| {
                ClipboardFault::Failed(format!("Navo cannot open {}", path.display()))
            })
        })
        .collect::<Result<_, _>>()?;
    let effect = match files.effect {
        Effect::Copy => ClipboardEffect::Copy,
        Effect::Move => ClipboardEffect::Move,
    };
    Ok(ClipboardFiles { items, effect })
}

impl FileClipboardService for ShellClipboard {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault> {
        rusty_shell::write_files(&to_shell(files)?).map_err(fault)
    }

    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault> {
        rusty_shell::read_files()
            .map_err(fault)?
            .map(|files| from_shell(&files))
            .transpose()
    }

    fn has_files(&self) -> bool {
        rusty_shell::has_files()
    }

    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault> {
        rusty_shell::clear_if_same(&to_shell(files)?).map_err(fault)
    }
}

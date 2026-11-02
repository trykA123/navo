use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, IDropSource,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILFindLastID, SHCreateDataObject, SHDoDragDrop};

use super::{DragError, DropOutcome};
use crate::pidl::Pidl;

fn shell_error(context: &'static str) -> impl Fn(windows::core::Error) -> DragError {
    move |error| DragError::Shell {
        context,
        message: error.message(),
    }
}

pub(super) fn data_object(paths: &[PathBuf]) -> Result<IDataObject, DragError> {
    let pidls = paths
        .iter()
        .map(|path| {
            Pidl::parse(path).map_err(|message| DragError::Path {
                path: path.clone(),
                message,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (first, first_path) = pidls.first().zip(paths.first()).ok_or(DragError::Empty)?;
    let parent = first.parent().ok_or_else(|| DragError::Path {
        path: first_path.clone(),
        message: "the shell returned no parent folder".to_owned(),
    })?;
    // SAFETY: the PIDLs outlive the call; ILFindLastID points into them and the data object copies them.
    unsafe {
        let children: Vec<*const ITEMIDLIST> = pidls
            .iter()
            .map(|pidl| ILFindLastID(pidl.as_ptr()).cast_const())
            .collect();
        SHCreateDataObject::<_, IDataObject>(
            Some(parent.as_ptr()),
            Some(children.as_slice()),
            None::<&IDataObject>,
        )
    }
    .map_err(shell_error("SHCreateDataObject"))
}

pub(super) fn drag_files(hwnd: isize, paths: &[PathBuf]) -> Result<DropOutcome, DragError> {
    let data = data_object(paths)?;
    let allowed = DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK;
    // SAFETY: runs on the thread that owns hwnd; the data object lives until the call returns.
    let effect = unsafe {
        SHDoDragDrop(
            Some(HWND(hwnd as *mut core::ffi::c_void)),
            &data,
            None::<&IDropSource>,
            allowed,
        )
    }
    .map_err(shell_error("SHDoDragDrop"))?;
    Ok(outcome(effect))
}

fn outcome(effect: DROPEFFECT) -> DropOutcome {
    if effect.contains(DROPEFFECT_MOVE) {
        DropOutcome::Moved
    } else if effect.contains(DROPEFFECT_COPY) {
        DropOutcome::Copied
    } else if effect.contains(DROPEFFECT_LINK) {
        DropOutcome::Linked
    } else {
        DropOutcome::NoEffect
    }
}

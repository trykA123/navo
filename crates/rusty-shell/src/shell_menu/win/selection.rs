use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, ILFindLastID, IShellFolder, SHBindToObject, SHBindToParent,
};

use super::shell_error;
use crate::pidl::Pidl;
use crate::shell_menu::MenuError;

fn parse(path: &Path) -> Result<Pidl, MenuError> {
    Pidl::parse(path).map_err(|message| MenuError::Path {
        path: path.to_path_buf(),
        message,
    })
}

pub(super) fn context_menu(paths: &[PathBuf], owner: HWND) -> Result<IContextMenu, MenuError> {
    let pidls = paths
        .iter()
        .map(|path| parse(path))
        .collect::<Result<Vec<_>, _>>()?;
    let first = pidls.first().ok_or(MenuError::Empty)?;
    // SAFETY: the PIDLs outlive the call; ILFindLastID returns pointers into them.
    unsafe {
        let folder: IShellFolder =
            SHBindToParent(first.as_ptr(), None).map_err(shell_error("SHBindToParent"))?;
        let children: Vec<*const ITEMIDLIST> = pidls
            .iter()
            .map(|pidl| ILFindLastID(pidl.as_ptr()).cast_const())
            .collect();
        folder
            .GetUIObjectOf(owner, &children, None)
            .map_err(shell_error("IShellFolder::GetUIObjectOf"))
    }
}

pub(super) fn background_menu(folder: &Path, owner: HWND) -> Result<IContextMenu, MenuError> {
    let pidl = parse(folder)?;
    // SAFETY: the PIDL outlives both calls; the folder object owns its own references.
    unsafe {
        let shell_folder: IShellFolder =
            SHBindToObject(None, pidl.as_ptr(), None).map_err(shell_error("SHBindToObject"))?;
        shell_folder
            .CreateViewObject(owner)
            .map_err(shell_error("IShellFolder::CreateViewObject"))
    }
}

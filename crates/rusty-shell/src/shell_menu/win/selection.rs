use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, ILFindLastID, IShellFolder, SHBindToObject, SHBindToParent, SHParseDisplayName,
};
use windows::core::HSTRING;

use super::shell_error;
use crate::shell_menu::MenuError;

struct Pidl(NonNull<ITEMIDLIST>);

impl Pidl {
    fn parse(path: &Path) -> Result<Self, MenuError> {
        let path_error = |message: String| MenuError::Path {
            path: path.to_path_buf(),
            message,
        };
        let absolute = std::path::absolute(path).map_err(|e| path_error(e.to_string()))?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: a valid wide string in; the shell allocates the PIDL that Drop frees.
        unsafe {
            SHParseDisplayName(
                &HSTRING::from(absolute.as_os_str()),
                None,
                &mut raw,
                0,
                None,
            )
        }
        .map_err(|e| path_error(e.message()))?;
        NonNull::new(raw)
            .map(Self)
            .ok_or_else(|| path_error("the shell returned no item".to_owned()))
    }

    fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0.as_ptr()
    }
}

impl Drop for Pidl {
    fn drop(&mut self) {
        // SAFETY: the PIDL came from SHParseDisplayName (CoTaskMemAlloc) and is freed once.
        unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
    }
}

pub(super) fn context_menu(paths: &[PathBuf], owner: HWND) -> Result<IContextMenu, MenuError> {
    let pidls = paths
        .iter()
        .map(|path| Pidl::parse(path))
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
    let pidl = Pidl::parse(folder)?;
    // SAFETY: the PIDL outlives both calls; the folder object owns its own references.
    unsafe {
        let shell_folder: IShellFolder =
            SHBindToObject(None, pidl.as_ptr(), None).map_err(shell_error("SHBindToObject"))?;
        shell_folder
            .CreateViewObject(owner)
            .map_err(shell_error("IShellFolder::CreateViewObject"))
    }
}

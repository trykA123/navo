use std::path::Path;
use std::ptr::NonNull;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILClone, ILRemoveLastID, SHParseDisplayName};
use windows::core::HSTRING;

pub(crate) struct Pidl(NonNull<ITEMIDLIST>);

impl Pidl {
    pub(crate) fn parse(path: &Path) -> Result<Self, String> {
        let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
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
        .map_err(|e| e.message())?;
        NonNull::new(raw)
            .map(Self)
            .ok_or_else(|| "the shell returned no item".to_owned())
    }

    pub(crate) fn parent(&self) -> Option<Self> {
        // SAFETY: ILClone copies a valid PIDL into shell memory (CoTaskMemAlloc) that Drop frees.
        let copy = NonNull::new(unsafe { ILClone(self.as_ptr()) }).map(Self)?;
        // SAFETY: the copy is owned here; ILRemoveLastID only shortens it in place.
        unsafe { ILRemoveLastID(Some(copy.0.as_ptr())) }
            .as_bool()
            .then_some(copy)
    }

    pub(crate) fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0.as_ptr()
    }
}

impl Drop for Pidl {
    fn drop(&mut self) {
        // SAFETY: the PIDL came from the shell (CoTaskMemAlloc) and is freed once.
        unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
    }
}

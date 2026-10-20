#![allow(unsafe_code, reason = "Windows shell links require COM calls")]

use std::io;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;

use rusty_core::FsError;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

use super::path_error;

pub(super) struct Shortcut {
    directory: tempfile::TempDir,
}

impl Shortcut {
    pub(super) fn new(target_dir: &Path, source: &Path) -> Result<Self, FsError> {
        let _apartment = Apartment::new().map_err(|e| com_error(e, source))?;
        let directory = tempfile::Builder::new()
            .prefix(".ferrite-link-")
            .tempdir_in(target_dir)
            .map_err(|e| path_error(&e, target_dir))?;
        save(source, &directory.path().join("shortcut.lnk"))?;
        Ok(Self { directory })
    }

    pub(super) fn publish(&self, at: &Path) -> io::Result<()> {
        super::super::win_replace::rename_new(&self.directory.path().join("shortcut.lnk"), at)
    }
}

struct Apartment {
    owned: bool,
    _thread: PhantomData<Rc<()>>,
}

impl Apartment {
    fn new() -> windows::core::Result<Self> {
        // SAFETY: COM is initialized on this thread and balanced by this thread-bound guard.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let owned = if result == RPC_E_CHANGED_MODE {
            false
        } else {
            result.ok()?;
            true
        };
        Ok(Self {
            owned,
            _thread: PhantomData,
        })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: the guard balances one successful initialization on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

fn save(source: &Path, at: &Path) -> Result<(), FsError> {
    let source_wide = wide(source).map_err(|e| path_error(&e, source))?;
    let at_wide = wide(at).map_err(|e| path_error(&e, at))?;
    // SAFETY: COM is initialized; both NUL-terminated buffers outlive these synchronous calls.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| com_error(e, at))?;
        link.SetPath(PCWSTR(source_wide.as_ptr()))
            .map_err(|e| com_error(e, source))?;
        let file: IPersistFile = link.cast().map_err(|e| com_error(e, at))?;
        file.Save(PCWSTR(at_wide.as_ptr()), false)
            .map_err(|e| com_error(e, at))
    }
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(io::ErrorKind::InvalidFilename.into());
    }
    units.push(0);
    Ok(units)
}

fn com_error(error: windows::core::Error, path: &Path) -> FsError {
    let code = error.code().0;
    let win32_code = code & 65_535;
    let code = if windows::core::HRESULT::from_win32(win32_code.cast_unsigned()).0 == code {
        win32_code
    } else {
        code
    };
    path_error(&io::Error::from_raw_os_error(code), path)
}

#[cfg(test)]
mod tests;

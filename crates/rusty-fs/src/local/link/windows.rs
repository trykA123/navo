#![allow(unsafe_code, reason = "Windows shell links require COM calls")]

use std::ffi::OsString;
use std::io;
use std::marker::PhantomData;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusty_core::{FsError, Location};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
use windows::core::{Interface, PCWSTR};

use super::path_error;

pub(super) struct Shortcut {
    directory: tempfile::TempDir,
}

impl Shortcut {
    pub(super) fn new(
        target_dir: &Path,
        source: &Path,
        at: Option<&Location>,
    ) -> Result<Self, FsError> {
        let _apartment = Apartment::new().map_err(|e| com_error(e, source))?;
        let directory = tempfile::Builder::new()
            .prefix(".ferrite-link-")
            .tempdir_in(target_dir)
            .map_err(|e| path_error(&e, target_dir))?;
        save(
            source,
            &directory.path().join("shortcut.lnk"),
            at,
            target_dir,
        )?;
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

fn save(source: &Path, temp: &Path, at: Option<&Location>, folder: &Path) -> Result<(), FsError> {
    let source_wide = wide(source).map_err(|e| path_error(&e, source))?;
    let saved_wide = wide(temp).map_err(|e| path_error(&e, temp))?;
    let saving = |error: windows::core::Error| match at {
        Some(at) => super::super::io_error(&com_io(error), at),
        None => com_error(error, folder),
    };
    // SAFETY: COM is initialized; both NUL-terminated buffers outlive these synchronous calls.
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(saving)?;
        link.SetPath(PCWSTR(source_wide.as_ptr()))
            .map_err(|e| com_error(e, source))?;
        let file: IPersistFile = link.cast().map_err(saving)?;
        file.Save(PCWSTR(saved_wide.as_ptr()), false)
            .map_err(saving)
    }
}

pub(super) fn read_target(path: &Path) -> io::Result<PathBuf> {
    let _apartment = Apartment::new().map_err(com_io)?;
    let path = wide(path)?;
    let mut buffer = vec![0; 32_768];
    // SAFETY: COM is initialized; the path and output buffers outlive these synchronous calls.
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(com_io)?;
        let file: IPersistFile = link.cast().map_err(com_io)?;
        file.Load(PCWSTR(path.as_ptr()), STGM_READ)
            .map_err(com_io)?;
        link.GetPath(
            &mut buffer,
            std::ptr::null_mut(),
            SLGP_RAWPATH.0.cast_unsigned(),
        )
        .map_err(com_io)?;
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(io::ErrorKind::InvalidData)?;
    Ok(PathBuf::from(OsString::from_wide(&buffer[..end])))
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(io::ErrorKind::InvalidFilename.into());
    }
    units.push(0);
    Ok(units)
}

fn com_io(error: windows::core::Error) -> io::Error {
    let code = error.code().0;
    let win32_code = code & 65_535;
    let code = if windows::core::HRESULT::from_win32(win32_code.cast_unsigned()).0 == code {
        win32_code
    } else {
        code
    };
    io::Error::from_raw_os_error(code)
}

fn com_error(error: windows::core::Error, path: &Path) -> FsError {
    path_error(&com_io(error), path)
}

#[cfg(test)]
mod tests;

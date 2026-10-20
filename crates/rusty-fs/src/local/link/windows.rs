#![allow(unsafe_code, reason = "Windows shell links require COM calls")]

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusty_core::{FsError, Location};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Storage::FileSystem::GetLongPathNameW;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistStream,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, IStream_Read, IStream_Reset, IStream_Size, SHCreateMemStream, SLGP_RAWPATH,
    ShellLink,
};
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

const MOST_SHORTCUT_BYTES: u64 = 1 << 20;

fn save(source: &Path, temp: &Path, at: Option<&Location>, folder: &Path) -> Result<(), FsError> {
    let source_wide = wide(source).map_err(|e| path_error(&e, source))?;
    let saving = |error: io::Error| match at {
        Some(at) => super::super::io_error(&error, at),
        None => path_error(&error, folder),
    };
    // SAFETY: COM is initialized; the NUL-terminated source and the byte buffer outlive these
    // synchronous calls, and the buffer holds exactly the `length` bytes read into it.
    let bytes = unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| saving(com_io(e)))?;
        link.SetPath(PCWSTR(source_wide.as_ptr()))
            .map_err(|e| com_error(e, source))?;
        let persist: IPersistStream = link.cast().map_err(|e| saving(com_io(e)))?;
        let stream =
            SHCreateMemStream(None).ok_or_else(|| saving(io::ErrorKind::OutOfMemory.into()))?;
        persist.Save(&stream, true).map_err(|e| saving(com_io(e)))?;
        let size = IStream_Size(&stream).map_err(|e| saving(com_io(e)))?;
        let length = u32::try_from(size).map_err(|_| saving(io::ErrorKind::FileTooLarge.into()))?;
        let mut bytes = vec![
            0_u8;
            usize::try_from(size)
                .map_err(|_| saving(io::ErrorKind::FileTooLarge.into()))?
        ];
        IStream_Reset(&stream).map_err(|e| saving(com_io(e)))?;
        IStream_Read(&stream, bytes.as_mut_ptr().cast(), length).map_err(|e| saving(com_io(e)))?;
        bytes
    };
    write_new(temp, &bytes).map_err(saving)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(super) fn read_target(path: &Path) -> io::Result<PathBuf> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MOST_SHORTCUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MOST_SHORTCUT_BYTES {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let _apartment = Apartment::new().map_err(com_io)?;
    let mut buffer = vec![0; 32_768];
    // SAFETY: COM is initialized; the byte and output buffers outlive these synchronous calls.
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(com_io)?;
        let persist: IPersistStream = link.cast().map_err(com_io)?;
        let stream = SHCreateMemStream(Some(bytes.as_slice())).ok_or(io::ErrorKind::OutOfMemory)?;
        persist.Load(&stream).map_err(com_io)?;
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

pub(super) fn same_path(found: &Path, wanted: &Path) -> bool {
    let (found, wanted) = (long_form(found), long_form(wanted));
    found == wanted
        || found
            .to_string_lossy()
            .eq_ignore_ascii_case(&wanted.to_string_lossy())
}

fn long_form(path: &Path) -> PathBuf {
    let Ok(units) = wide(path) else {
        return path.to_path_buf();
    };
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: the path is NUL-terminated and both buffers outlive this synchronous call.
    let length = unsafe { GetLongPathNameW(PCWSTR(units.as_ptr()), Some(&mut buffer)) };
    match usize::try_from(length) {
        Ok(length) if length > 0 && length < buffer.len() => {
            PathBuf::from(OsString::from_wide(&buffer[..length]))
        }
        _ => path.to_path_buf(),
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

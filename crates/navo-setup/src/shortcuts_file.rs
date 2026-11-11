//! Windows link-file handles: reparse-safe opens, link bytes and delete dispositions.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistStream,
};
use windows::Win32::UI::Shell::{IShellLinkW, SHCreateMemStream, SLGP_RAWPATH, ShellLink};
use windows::core::Interface;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FileAttributeTagInfo, FileDispositionInfo, GetFileInformationByHandleEx, GetLongPathNameW,
    OPEN_EXISTING, ReadFile, SetFileInformationByHandle,
};

use super::inside;

/// Opens a link without following its final reparse point and accepts only a regular file.
pub(super) fn open_regular(path: &Path) -> io::Result<Option<Owned>> {
    let path = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and the buffers live for the call.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            DELETE | FILE_READ_ATTRIBUTES | FILE_GENERIC_READ,
            FILE_SHARE_READ,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let owned = Owned(handle);
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    let size = u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>()).map_err(io::Error::other)?;
    // SAFETY: the handle is live and `tag` is a writable struct of the size passed.
    if unsafe {
        GetFileInformationByHandleEx(
            owned.0,
            FileAttributeTagInfo,
            std::ptr::from_mut(&mut tag).cast::<c_void>(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let reparse = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0;
    let directory = tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    Ok((!reparse && !directory).then_some(owned))
}

/// Largest link file read, in bytes; real .lnk files are a few KiB.
pub(super) const LINK_LIMIT: usize = 1 << 20;

/// Reads the whole link through the held handle, so the parsed bytes are the file deleted.
///
/// Fails on a read error or a file larger than `LINK_LIMIT`.
pub(super) fn read_link(file: &Owned) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let mut read = 0u32;
        let len = u32::try_from(chunk.len()).map_err(io::Error::other)?;
        // SAFETY: the handle is live and `chunk` is writable for `len` bytes during the call.
        if unsafe {
            ReadFile(
                file.0,
                chunk.as_mut_ptr(),
                len,
                &raw mut read,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if read == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(
            chunk
                .get(..usize::try_from(read).map_err(io::Error::other)?)
                .unwrap_or_default(),
        );
        if bytes.len() > LINK_LIMIT {
            return Err(io::Error::other("link file too large"));
        }
    }
}

/// Parses link bytes through IPersistStream and compares the raw target with `root` by name.
///
/// Loading from memory avoids a second open, which our DELETE handle would block.
pub(super) fn target_inside(root: &Path, link_bytes: &[u8]) -> bool {
    let Ok(apartment) = Apartment::new() else {
        return false;
    };
    let mut buffer = vec![0u16; 32_768];
    let target = (|| -> windows::core::Result<PathBuf> {
        // SAFETY: COM is initialized for this thread by `apartment`, which outlives this call.
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
        let persist: IPersistStream = link.cast()?;
        // SAFETY: the byte slice outlives the call, which copies it into a new stream.
        let stream = unsafe { SHCreateMemStream(Some(link_bytes)) }.ok_or_else(|| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
        })?;
        // SAFETY: the stream and buffer live across these calls; `GetPath` gets a fixed buffer,
        // SLGP_RAWPATH and no Resolve step, so it never searches the disk.
        unsafe {
            persist.Load(&stream)?;
            link.GetPath(
                &mut buffer,
                std::ptr::null_mut(),
                SLGP_RAWPATH.0.cast_unsigned(),
            )?;
        }
        let end = buffer.iter().position(|unit| *unit == 0).ok_or_else(|| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
        })?;
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
    })();
    drop(apartment);
    // Compare long names: a root or target spelled with 8.3 parts (RUNNER~1) names the same folder.
    target.is_ok_and(|target| inside(&long_form(root), &long_form(&target)))
}

/// The path with 8.3 short parts expanded to long names. A missing tail (a link target not yet
/// installed) is kept as written below its longest existing ancestor; the path stays unchanged
/// when no ancestor expands.
fn long_form(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut tail = Vec::new();
    loop {
        if let Some(mut long) = long_existing(existing) {
            long.extend(tail.iter().rev());
            return long;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The long-name form of an existing path; `None` when it does not exist or expansion fails.
fn long_existing(path: &Path) -> Option<PathBuf> {
    let wide = wide_z(path);
    // SAFETY: `wide` is NUL-terminated; a null output with zero length asks for the needed size.
    let needed = unsafe { GetLongPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    let size = usize::try_from(needed).ok().filter(|&size| size > 0)?;
    let mut buffer = vec![0u16; size];
    // SAFETY: `buffer` has `needed` writable units and `wide` stays NUL-terminated.
    let length = unsafe { GetLongPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };
    match usize::try_from(length) {
        Ok(length) if length > 0 && length < size => Some(PathBuf::from(
            std::ffi::OsString::from_wide(&buffer[..length]),
        )),
        _ => None,
    }
}

/// Marks a handle's file for deletion when the handle closes.
pub(super) fn set_disposition(file: &Owned) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let size = u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).map_err(io::Error::other)?;
    // SAFETY: the handle is live and `info` is a readable struct of the size passed.
    if unsafe {
        SetFileInformationByHandle(
            file.0,
            FileDispositionInfo,
            std::ptr::from_ref(&info).cast::<c_void>(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A NUL-terminated UTF-16 path for the Win32 and COM calls.
pub(super) fn wide_z(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// One COM apartment initialization, released when dropped on the same thread.
pub(super) struct Apartment {
    owned: bool,
}

impl Apartment {
    /// Initializes apartment-threaded COM; `RPC_E_CHANGED_MODE` is accepted without ownership.
    pub(super) fn new() -> windows::core::Result<Self> {
        // SAFETY: COM is initialized on this thread and balanced by this thread-bound guard.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { owned: false });
        }
        result.ok()?;
        Ok(Self { owned: true })
    }
}

impl Drop for Apartment {
    /// Balances one successful initialization on this thread.
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: the guard owns one successful CoInitializeEx on this same thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// A Win32 handle that closes on drop.
pub(super) struct Owned(HANDLE);

impl Drop for Owned {
    /// Closes the owned link handle.
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}

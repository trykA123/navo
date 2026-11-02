use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, MapViewOfFile, PAGE_READONLY, UnmapViewOfFile,
};

pub(super) fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)?;
    let mut bytes = Vec::new();
    match file.read_to_end(&mut bytes) {
        Ok(_) => Ok(bytes),
        Err(error) if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) => {
            mapped_bytes(&file)
        }
        Err(error) => Err(error),
    }
}

fn mapped_bytes(file: &File) -> io::Result<Vec<u8>> {
    let size = usize::try_from(file.metadata()?.len()).unwrap();
    assert!(size > 0 && size <= isize::MAX as usize);
    // SAFETY: file is open for reading; the unnamed mapping covers its current size.
    let mapping = unsafe {
        CreateFileMappingW(
            file.as_raw_handle(),
            std::ptr::null(),
            PAGE_READONLY,
            0,
            0,
            std::ptr::null(),
        )
    };
    if mapping.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileMappingW returned a new handle owned by this scope.
    let mapping = unsafe { OwnedHandle::from_raw_handle(mapping) };
    // SAFETY: the live mapping permits a read-only view of size bytes from offset zero.
    let view = unsafe { MapViewOfFile(mapping.as_raw_handle(), FILE_MAP_READ, 0, 0, size) };
    if view.Value.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the view spans size bytes; Windows byte-range locks do not restrict mapped reads.
    let bytes = unsafe { std::slice::from_raw_parts(view.Value.cast::<u8>(), size) }.to_vec();
    // SAFETY: view is live and no reference into it remains after the copy.
    if unsafe { UnmapViewOfFile(view) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes)
}

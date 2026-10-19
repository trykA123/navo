#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for MoveFileExW with write-through"
)]

use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::Storage::FileSystem::{
    MOVE_FILE_FLAGS, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

pub(super) fn replace(from: &Path, to: &Path) -> io::Result<()> {
    match move_over(from, to) {
        Err(denied) if denied.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) => {
            replace_read_only(from, to, denied)
        }
        other => other,
    }
}

fn replace_read_only(from: &Path, to: &Path, denied: io::Error) -> io::Result<()> {
    let Ok(metadata) = fs::symlink_metadata(to) else {
        return Err(denied);
    };
    let mut permissions = metadata.permissions();
    if !metadata.is_file() || !permissions.readonly() {
        return Err(denied);
    }
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "on Windows this clears only the read-only attribute"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(to, permissions)?;
    move_over(from, to).inspect_err(|_| {
        let _ = fs::set_permissions(to, metadata.permissions());
    })
}

pub(super) fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    move_with(from, to, MOVEFILE_WRITE_THROUGH)
}

fn move_over(from: &Path, to: &Path) -> io::Result<()> {
    move_with(from, to, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
}

fn move_with(from: &Path, to: &Path, flags: MOVE_FILE_FLAGS) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both buffers are NUL-terminated UTF-16 paths that outlive this synchronous call.
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

use std::io;
use std::path::Path;

#[cfg(target_os = "linux")]
pub(super) fn available(path: &Path) -> io::Result<Option<u64>> {
    let stat = rustix::fs::statvfs(path)?;
    let bytes = stat
        .f_bavail
        .checked_mul(stat.f_frsize)
        .ok_or_else(|| io::Error::other("available filesystem size overflow"))?;
    Ok(Some(bytes))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the Windows disk-space API writes an output byte count"
)]
pub(super) fn available(path: &Path) -> io::Result<Option<u64>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut bytes = 0;
    // SAFETY: the terminated path and writable output live through the synchronous query.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut bytes,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Some(bytes))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(super) fn available(_path: &Path) -> io::Result<Option<u64>> {
    Ok(None)
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn native_space_observation_requires_an_existing_path() {
        let root = tempfile::tempdir().unwrap();
        assert!(available(root.path()).unwrap().is_some());
        assert!(available(&root.path().join("missing")).is_err());
    }
}

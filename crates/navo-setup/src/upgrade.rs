//! Upgrade preparation: package staging, Update.exe replacement and apply arguments.

use std::ffi::OsString;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_DELETE_ON_CLOSE,
};

#[cfg(windows)]
use crate::payload::Payload;
#[cfg(windows)]
use crate::probe::pin::Pin;

#[cfg(windows)]
#[path = "upgrade_windows.rs"]
mod platform;

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;

/// The install root passed to `apply_args`.
pub type Root = PathBuf;

/// The package written into `<root>\packages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub path: PathBuf,
}

/// Builds IW3's exact `Update.exe apply` array, followed by `--log`.
pub fn apply_args(root: &Root, staged: &Staged, log: &Path) -> Vec<OsString> {
    vec![
        root.join("Update.exe").into_os_string(),
        "apply".into(),
        "--package".into(),
        staged.path.clone().into_os_string(),
        "--silent".into(),
        "--norestart".into(),
        "--root".into(),
        root.clone().into_os_string(),
        "--packageDir".into(),
        root.join("packages").into_os_string(),
        "--log".into(),
        log.as_os_str().to_os_string(),
    ]
}

/// Proves the held root accepts a new file, with a delete-on-close probe.
#[cfg(windows)]
pub fn writable(pin: &Pin) -> io::Result<bool> {
    let root = pin
        .root()
        .ok_or_else(|| io::Error::other("pin has no root"))?;
    let path = root.join(format!(".navo-setup-probe-{}", random_hex()?));
    let path = wide(&path);
    // SAFETY: the path is NUL-terminated, the buffers are live, and no security
    // attributes or template are used.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE | DELETE,
            0,
            std::ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_DELETE_ON_CLOSE,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Ok(false);
    }
    drop(Owned(handle));
    Ok(true)
}

/// Writes the embedded nupkg into packages and renames it into place.
#[cfg(windows)]
pub fn stage_package(pin: &Pin, payload: &Payload) -> io::Result<Staged> {
    let root = pin
        .root()
        .ok_or_else(|| io::Error::other("pin has no root"))?;
    Ok(Staged {
        path: platform::stage_package(root, payload)?,
    })
}

/// Replaces Update.exe with the nupkg's single Squirrel.exe entry.
#[cfg(windows)]
pub fn replace_update_exe(pin: &Pin, payload: &Payload) -> io::Result<[u8; 32]> {
    let root = pin
        .root()
        .ok_or_else(|| io::Error::other("pin has no root"))?;
    platform::replace_update_exe(root, payload)
}

/// Returns 32 random hex characters for the write probe.
#[cfg(windows)]
fn random_hex() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    // SAFETY: the buffer is writable and the system-preferred RNG needs no algorithm handle.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            16,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "BCryptGenRandom failed: 0x{status:08X}"
        )));
    }
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

/// A NUL-terminated UTF-16 path for the write probe.
#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A Win32 handle that closes on drop.
#[cfg(windows)]
struct Owned(HANDLE);

#[cfg(windows)]
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}

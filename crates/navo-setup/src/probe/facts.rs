//! Windows facts probe: drive kind, subst mapping, free space, write access and elevation.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "drive types, device maps, token elevation and the write probe require Win32 FFI"
)]

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_DELETE_ON_CLOSE,
    GetDiskFreeSpaceExW, GetDriveTypeW, QueryDosDeviceW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE};

use crate::placement::{Chain, DriveKind, Facts};

/// The largest target `QueryDosDeviceW` can return, in UTF-16 units.
const TARGET_UNITS: usize = 32_768;

/// The DOS-device prefix that marks a `subst` drive.
const SUBST_PREFIX: [u16; 4] = [0x5c, 0x3f, 0x3f, 0x5c];

/// The write probe's name prefix.
const PROBE_PREFIX: &str = ".navo-setup-probe-";

/// Reads the drive facts and proves the nearest existing folder is writable.
pub fn facts(chain: &Chain) -> io::Result<Facts> {
    let root = chain
        .existing
        .first()
        .ok_or_else(|| io::Error::other("chain has no drive root"))?;
    let nearest = chain
        .existing
        .last()
        .ok_or_else(|| io::Error::other("chain has no existing folder"))?;
    Ok(Facts {
        drive: drive_kind(&root.path)?,
        subst: subst_drive(&root.path),
        writable: write_probe(&nearest.path)?,
        free: free_space(&nearest.path)?,
    })
}

/// Reads `TokenElevation`; an error is returned for the caller to treat as elevated (IW13).
pub fn elevated() -> io::Result<bool> {
    let mut raw: HANDLE = std::ptr::null_mut();
    // SAFETY: the process pseudo-handle is valid and `raw` is a writable handle pointer.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Owned(raw);

    let mut info = TOKEN_ELEVATION::default();
    let size = u32::try_from(size_of::<TOKEN_ELEVATION>()).map_err(io::Error::other)?;
    let mut returned = 0u32;
    let info_ptr = std::ptr::from_mut(&mut info).cast::<c_void>();
    // SAFETY: the token handle is live and `info` is a writable struct of the size passed.
    let result =
        unsafe { GetTokenInformation(token.0, TokenElevation, info_ptr, size, &raw mut returned) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info.TokenIsElevated != 0)
}

/// Maps `GetDriveTypeW` to the placement drive kind.
fn drive_kind(root: &Path) -> io::Result<DriveKind> {
    let root = wide_z(&drive_root(root)?);
    // SAFETY: the root path is NUL-terminated and live for the call.
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    Ok(match kind {
        DRIVE_FIXED => DriveKind::Fixed,
        DRIVE_REMOVABLE => DriveKind::Removable,
        DRIVE_REMOTE => DriveKind::Remote,
        _ => DriveKind::Other,
    })
}

/// Whether `QueryDosDeviceW` maps the drive through `\??\`; any error counts as `subst`.
fn subst_drive(root: &Path) -> bool {
    let Ok(letter) = drive_letter(root) else {
        return true;
    };
    let device = wide_z(Path::new(&format!("{letter}:")));
    let mut target = vec![0u16; TARGET_UNITS];
    let Ok(capacity) = u32::try_from(TARGET_UNITS) else {
        return true;
    };
    // SAFETY: the device name is NUL-terminated and `target` has `capacity` writable units.
    let length = unsafe { QueryDosDeviceW(device.as_ptr(), target.as_mut_ptr(), capacity) };
    if length == 0 || length > capacity {
        return true;
    }
    target.starts_with(&SUBST_PREFIX)
}

/// Creates and closes a `CREATE_NEW` probe file; false means the folder is not writable.
fn write_probe(parent: &Path) -> io::Result<bool> {
    let path = parent.join(format!("{PROBE_PREFIX}{}", random_hex()?));
    let path = wide_z(&path);
    // SAFETY: the path is NUL-terminated, the buffer is live, and no template or security
    // attributes are used.
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

/// Reads the free space available to the caller at `path`.
fn free_space(path: &Path) -> io::Result<u64> {
    let path = wide_z(path);
    let mut free = 0u64;
    // SAFETY: the path is NUL-terminated and `free` is a writable u64; the other outputs are unused.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}

/// Returns 32 random hex characters from the system RNG.
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

/// Reads the drive letter from a drive-root path.
fn drive_letter(root: &Path) -> io::Result<char> {
    let text = root.to_string_lossy();
    let mut chars = text.chars();
    let Some(letter) = chars.next() else {
        return Err(io::Error::other("chain root has no drive letter"));
    };
    if !letter.is_ascii_alphabetic() || chars.next() != Some(':') {
        return Err(io::Error::other("chain root is not a drive path"));
    }
    Ok(letter)
}

/// Builds the `X:\` root `GetDriveTypeW` expects.
fn drive_root(root: &Path) -> io::Result<PathBuf> {
    Ok(PathBuf::from(format!("{}:\\", drive_letter(root)?)))
}

/// A NUL-terminated UTF-16 path for the Win32 calls.
fn wide_z(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A Win32 handle that closes on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}

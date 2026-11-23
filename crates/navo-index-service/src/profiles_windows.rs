//! Windows side of the profile scope: registry reads of `ProfileList` and final paths behind
//! junctions, for the portable rules in `profiles.rs`.

use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_NAME_NORMALIZED, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, GetFinalPathNameByHandleW, OPEN_EXISTING, VOLUME_NAME_DOS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};

use super::profiles::ProfileSource;

/// Longest registry string value and longest extended path, in UTF-16 units.
const MAX_UNITS: usize = 32_767;
/// Longest registry key name, in UTF-16 units, plus its NUL.
const MAX_KEY_UNITS: usize = 256;

/// The real registry and file system.
pub(super) struct System;

/// Encodes `text` as a NUL-terminated UTF-16 string.
fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

impl ProfileSource for System {
    /// Reads a string value under `HKLM\key`; `RRF_RT_REG_SZ` without `RRF_NOEXPAND` also
    /// accepts `REG_EXPAND_SZ` and expands it.
    #[expect(unsafe_code, reason = "RegGetValueW is the registry read")]
    fn text(&self, key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut buffer = vec![0_u16; MAX_UNITS];
        let mut size = u32::try_from(MAX_UNITS * 2).ok()?;
        // SAFETY: names are NUL-terminated; `size` is the buffer length in bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        // An expanded REG_EXPAND_SZ reports the unexpanded size, so the text ends at its NUL.
        let units = (usize::try_from(size).ok()? / 2).min(MAX_UNITS);
        let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(units);
        Some(String::from_utf16_lossy(buffer.get(..end)?))
    }

    /// Lists the subkeys of `HKLM\key`; any status other than the end of the list fails.
    #[expect(
        unsafe_code,
        reason = "RegOpenKeyExW and RegEnumKeyExW are the registry list"
    )]
    fn subkeys(&self, key: &str) -> Result<Vec<String>, String> {
        let key = wide(key);
        let mut handle: HKEY = null_mut();
        // SAFETY: the key name is NUL-terminated and `handle` is a valid out pointer.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                0,
                KEY_READ,
                &raw mut handle,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!(
                "opening the profile list failed with status {status}"
            ));
        }
        let listed = enumerate(handle);
        // SAFETY: `handle` was opened above and is closed once.
        unsafe { RegCloseKey(handle) };
        listed
    }

    /// Opens `path` as a folder (backup semantics, no access rights) and asks for its final
    /// DOS path; `None` when it cannot be opened or the answer does not fit.
    #[expect(
        unsafe_code,
        reason = "CreateFileW and GetFinalPathNameByHandleW resolve junctions"
    )]
    fn final_path(&self, path: &str) -> Option<String> {
        let path = wide(path);
        // SAFETY: the path is NUL-terminated; no security attributes or template are passed.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut buffer = vec![0_u16; MAX_UNITS];
        // A capacity of 0 makes the call fail, which reads as "no final path".
        let capacity = u32::try_from(buffer.len()).unwrap_or(0);
        // SAFETY: the handle is open and the buffer holds `capacity` units.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                capacity,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        // SAFETY: the handle was opened above and is closed once.
        unsafe { CloseHandle(handle) };
        let length = usize::try_from(length).ok()?;
        let path = buffer
            .get(..length)
            .filter(|_| length > 0 && length < buffer.len())?;
        Some(String::from_utf16_lossy(path))
    }
}

/// Reads every subkey name of the open key `handle`.
#[expect(unsafe_code, reason = "RegEnumKeyExW is the registry list")]
fn enumerate(handle: HKEY) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for index in 0_u32.. {
        let mut name = [0_u16; MAX_KEY_UNITS];
        let mut units = u32::try_from(name.len()).map_err(|error| error.to_string())?;
        // SAFETY: `name` holds `units` units; the class and time outputs are not requested.
        let status = unsafe {
            RegEnumKeyExW(
                handle,
                index,
                name.as_mut_ptr(),
                &raw mut units,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        match status {
            ERROR_SUCCESS => {
                let units = usize::try_from(units).map_err(|error| error.to_string())?;
                names.push(String::from_utf16_lossy(name.get(..units).unwrap_or(&name)));
            }
            ERROR_NO_MORE_ITEMS => return Ok(names),
            other => {
                return Err(format!(
                    "listing profile {index} failed with status {other}"
                ));
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the SID of the account running the test from `whoami /user /fo csv /nh`.
    fn current_sid() -> String {
        let output = std::process::Command::new("whoami")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .expect("whoami runs on every Windows");
        let text = String::from_utf8_lossy(&output.stdout);
        let sid = text.trim().rsplit(',').next().unwrap_or_default();
        sid.trim_matches('"').to_owned()
    }

    #[test]
    fn the_real_registry_gives_an_absolute_expanded_root_for_the_current_user() {
        let sid = current_sid();
        let scope = crate::profiles::scope_from(&sid, &System);
        assert!(scope.is_ok(), "the scope for {sid} must resolve: {scope:?}");
        let root = System
            .text(crate::profiles::PROFILE_LIST, "ProfilesDirectory")
            .expect("every Windows lists its profiles root");
        assert!(
            !root.contains('%'),
            "environment strings must be expanded: {root}"
        );
        assert!(
            std::path::Path::new(&root).is_absolute(),
            "{root} must be absolute"
        );
        let final_root = System.final_path(&root).expect("the profiles root exists");
        assert!(final_root.starts_with(r"\\?\"), "{final_root}");
    }
}

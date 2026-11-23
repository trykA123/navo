//! WSL distro entries for the drive list (NV11a). The pure part turns distro
//! names into `\\wsl.localhost\<distro>\` drives; the Windows part reads the names from the registry.

use super::model::{Drive, DriveKind};

const WSL_PREFIX: &str = r"\\wsl.localhost\";

impl Drive {
    /// True for a WSL distro entry. Its volume must not be probed: touching the
    /// share starts a stopped distro.
    #[must_use]
    pub fn is_wsl(&self) -> bool {
        self.root.starts_with(WSL_PREFIX)
    }
}

/// Builds one drive per usable distro name, in input order. Blank names,
/// case-insensitive duplicates and names with a path separator are skipped.
pub(super) fn wsl_drives(names: &[String]) -> Vec<Drive> {
    let mut seen: Vec<String> = Vec::new();
    let mut drives = Vec::new();
    for name in names.iter().map(|name| name.trim()) {
        let key = name.to_lowercase();
        if name.is_empty() || name.contains(['\\', '/']) || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        drives.push(Drive {
            root: format!(r"{WSL_PREFIX}{name}\"),
            letter: None,
            kind: DriveKind::Network,
            label: Some(name.to_owned()),
            remote: None,
            connected: true,
            space: None,
        });
    }
    drives
}

/// Reads installed distro names from `HKCU\...\Lxss\<guid>\DistributionName`.
/// A missing WSL (no key) gives an empty list, never an error.
#[cfg(windows)]
pub(super) fn installed_distros() -> Vec<String> {
    imp::subkeys(LXSS)
        .into_iter()
        .filter_map(|guid| {
            crate::registry::text(
                crate::registry::Hive::CurrentUser,
                &format!(r"{LXSS}\{guid}"),
                "DistributionName",
            )
        })
        .collect()
}

#[cfg(windows)]
const LXSS: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";

#[cfg(windows)]
mod imp {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    };

    // Registry key names are at most 255 characters.
    const NAME_UNITS: usize = 256;

    /// Lists the subkey names of `key` under HKCU; empty when the key cannot be opened.
    pub(super) fn subkeys(key: &str) -> Vec<String> {
        let wide: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        let mut handle: HKEY = null_mut();
        // SAFETY: `wide` is NUL-terminated and `handle` receives the opened key.
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide.as_ptr(),
                0,
                KEY_READ,
                &raw mut handle,
            )
        };
        if opened != ERROR_SUCCESS {
            return Vec::new();
        }
        let names = enumerate(handle);
        // SAFETY: `handle` came from a successful `RegOpenKeyExW` and is closed once.
        unsafe { RegCloseKey(handle) };
        names
    }

    /// Reads subkey names from an open key until the enumeration stops.
    fn enumerate(handle: HKEY) -> Vec<String> {
        let mut names = Vec::new();
        for index in 0u32.. {
            let mut buffer = [0u16; NAME_UNITS];
            let mut len = NAME_UNITS as u32;
            // SAFETY: `buffer` holds `len` units; unused outputs are null.
            let status = unsafe {
                RegEnumKeyExW(
                    handle,
                    index,
                    buffer.as_mut_ptr(),
                    &raw mut len,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            let used = (len as usize).min(NAME_UNITS);
            names.push(String::from_utf16_lossy(&buffer[..used]));
        }
        names
    }
}

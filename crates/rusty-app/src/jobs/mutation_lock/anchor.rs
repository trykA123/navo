use std::io;
use std::path::PathBuf;

use rusty_core::brand;

#[cfg(unix)]
pub(super) fn uid() -> u32 {
    // SAFETY: geteuid takes no arguments and returns the calling process's effective user ID.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
pub(super) fn directory() -> io::Result<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;
    let mut length = 16_384;
    loop {
        let mut storage = vec![0u8; length];
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: entry, result and storage remain writable and live for the native lookup.
        let code = unsafe {
            libc::getpwuid_r(
                uid(),
                entry.as_mut_ptr(),
                storage.as_mut_ptr().cast(),
                storage.len(),
                &raw mut result,
            )
        };
        if code == libc::ERANGE && length < 1_048_576 {
            length *= 2;
            continue;
        }
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        if result.is_null() {
            return Err(io::Error::other("user account is unavailable"));
        }
        // SAFETY: a successful getpwuid_r with non-null result initializes entry and its strings in storage.
        let home = unsafe { CStr::from_ptr(entry.assume_init().pw_dir) };
        let home = PathBuf::from(OsStr::from_bytes(home.to_bytes()));
        if !home.is_absolute() {
            return Err(io::Error::other(
                "user account has no absolute home directory",
            ));
        }
        return Ok(home
            .join(".local/share")
            .join(brand::SLUG)
            .join(runtime_name()));
    }
}

#[cfg(windows)]
pub(super) fn directory() -> io::Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};
    let mut text = std::ptr::null_mut();
    // SAFETY: the folder GUID and output pointer remain valid for the native lookup.
    let code = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            0,
            std::ptr::null_mut(),
            &raw mut text,
        )
    };
    if code < 0 {
        return Err(io::Error::other(format!(
            "local data directory unavailable: {code:#x}"
        )));
    }
    let mut length = 0;
    // SAFETY: successful SHGetKnownFolderPath returns a null-terminated allocation.
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    // SAFETY: length excludes the terminator and the allocation remains alive until freed below.
    let path = PathBuf::from(OsString::from_wide(unsafe {
        std::slice::from_raw_parts(text, length)
    }));
    // SAFETY: text is the allocation returned by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(text.cast()) };
    Ok(path.join(brand::NAME).join(runtime_name()))
}

fn runtime_name() -> String {
    format!("{}runtime", brand::MUTATIONS_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_is_a_reserved_navo_folder() {
        let anchor = directory().unwrap();
        let name = anchor.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(brand::MUTATIONS_PREFIX));
        assert!(brand::is_reserved_name(name));
    }

    #[test]
    fn anchor_probe() {
        println!("anchor:{}", directory().unwrap().display());
    }

    #[test]
    fn the_anchor_is_independent_of_session_and_profile_environment() {
        let expected = format!("anchor:{}", directory().unwrap().display());
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "jobs::mutation_lock::anchor::tests::anchor_probe",
                "--nocapture",
            ])
            .env("XDG_RUNTIME_DIR", root.path())
            .env("XDG_DATA_HOME", root.path())
            .env("LOCALAPPDATA", root.path())
            .env("HOME", root.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .any(|line| line == expected)
        );
    }
}

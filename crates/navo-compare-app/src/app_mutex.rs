//! The app's named Windows mutex. Inno Setup's `AppMutex` in `installer/sidewise.iss` looks for it,
//! so Setup and the uninstaller ask the user to close Sidewise first (P4 IN7, finding F2).

/// The mutex name; `installer/sidewise.iss` repeats it in `AppMutex=`. Never change it: installed
/// uninstallers look for this exact name. No `Global\` prefix, so a per-user install needs no admin.
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "only Windows has the installer")
)]
pub const NAME: &str = "Sidewise-7f3c2a91-5d4e-4b8a-9c61-0e2f7a3b8d54";

/// Creates or opens the named mutex and keeps it for the process lifetime (the handle is never
/// closed, so Windows drops it at exit). A second instance opens the same mutex. Does nothing
/// off Windows. Fails with the OS error when Windows refuses the mutex.
#[cfg(windows)]
#[allow(unsafe_code, reason = "Inno Setup looks for a named Win32 mutex")]
pub fn hold() -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = NAME.encode_utf16().chain([0]).collect();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that lives through the call; null attributes
    // mean default security, and the app does not take ownership (Inno only checks existence).
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Off Windows there is no installer to tell, so this does nothing.
#[cfg(not(windows))]
pub fn hold() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_names_the_same_mutex_and_the_folder() {
        let iss = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../installer/sidewise.iss"),
        )
        .expect("the installer script ships in the repo");
        let lines: Vec<&str> = iss.lines().map(str::trim).collect();
        assert!(
            lines.contains(&format!("AppMutex={NAME}").as_str()),
            "Setup and the uninstaller must wait for the mutex the app holds"
        );
        assert!(
            lines.contains(&"AlwaysShowDirOnReadyPage=yes"),
            "the Ready page must name the install folder"
        );
    }

    #[cfg(windows)]
    #[test]
    #[allow(unsafe_code, reason = "opens the mutex as Inno does")]
    fn mutex_is_open_while_the_app_runs() {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

        hold().expect("Windows creates the named mutex");
        let name: Vec<u16> = NAME.encode_utf16().chain([0]).collect();
        // SAFETY: `name` is a NUL-terminated UTF-16 string that lives through the call.
        let handle = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) };
        assert!(!handle.is_null(), "the installer finds the mutex by name");
        // SAFETY: `handle` is a valid handle this test owns; it is closed once.
        unsafe { CloseHandle(handle) };
    }
}

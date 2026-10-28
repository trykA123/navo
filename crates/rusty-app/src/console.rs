#[cfg(windows)]
pub fn attach() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: AttachConsole takes a process id and touches no memory we own; failure (no parent console) is harmless.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
pub fn attach() {}

#[cfg(windows)]
pub fn show_mode_mismatch(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };

    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = rusty_core::brand::NAME
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: Both strings are NUL-terminated and remain valid until the modal call returns; no owner is required.
    let _ = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        )
    };
}

#[cfg(not(windows))]
pub fn show_mode_mismatch(_message: &str) {}

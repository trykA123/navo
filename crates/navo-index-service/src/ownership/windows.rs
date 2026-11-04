#![allow(
    unsafe_code,
    reason = "reading protected directories requires Win32 token privileges"
)]

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::LUID;
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_BACKUP_NAME,
    SE_PRIVILEGE_ENABLED, SE_RESTORE_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(super) fn enable_privileges() -> io::Result<()> {
    let mut raw = ptr::null_mut();
    // SAFETY: the pseudo-handle is always valid and raw is writable handle storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken succeeded and transferred a uniquely owned token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    for name in [SE_RESTORE_NAME, SE_BACKUP_NAME] {
        let mut luid = LUID::default();
        // SAFETY: name is a static NUL-terminated privilege name and luid is writable.
        if unsafe { LookupPrivilegeValueW(ptr::null(), name, &mut luid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let state = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: token has ADJUST_PRIVILEGES access; state is live; no previous state is requested.
        let ok = unsafe {
            AdjustTokenPrivileges(
                token.as_raw_handle(),
                0,
                &state,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

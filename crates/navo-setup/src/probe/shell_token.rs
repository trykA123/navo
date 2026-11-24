//! Impersonation-level check for the shell probe: proves a thread token can do file work as the user.
//! `shell::enter_as_user` calls it after `SetThreadToken` (IW39, IW40).

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "reading the thread token's impersonation level requires Win32 FFI"
)]

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::{
    GetTokenInformation, SECURITY_IMPERSONATION_LEVEL, SecurityAnonymous, SecurityDelegation,
    SecurityIdentification, SecurityImpersonation, TOKEN_QUERY, TokenImpersonationLevel,
};
use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};

/// Reads the calling thread's impersonation level; fails when the thread has no token.
pub(super) fn thread_level() -> io::Result<SECURITY_IMPERSONATION_LEVEL> {
    let mut raw: HANDLE = null_mut();
    // SAFETY: the pseudo-handle names this thread; open-as-self checks against Setup's own token.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw) };
    if opened == 0 || raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh token handle this process owns; the wrapper closes it once.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut level: SECURITY_IMPERSONATION_LEVEL = SecurityAnonymous;
    let mut returned = 0u32;
    let size =
        u32::try_from(size_of::<SECURITY_IMPERSONATION_LEVEL>()).map_err(io::Error::other)?;
    // SAFETY: the token is live and `level` is a writable i32 of the size passed.
    let read = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenImpersonationLevel,
            (&raw mut level).cast::<c_void>(),
            size,
            &raw mut returned,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(level)
}

/// Accepts `SecurityImpersonation` or higher; a lower level is a permission error naming it.
pub(super) fn require_impersonation(level: SECURITY_IMPERSONATION_LEVEL) -> io::Result<()> {
    if level >= SecurityImpersonation {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "Windows lowered the desktop user's token to {} (needs impersonation)",
            level_name(level)
        ),
    ))
}

/// The Win32 name of an impersonation level, for error text.
fn level_name(level: SECURITY_IMPERSONATION_LEVEL) -> String {
    // A lookup table, not a `match`: the Win32 names are not upper case, so clippy refuses them as patterns.
    let names = [
        (SecurityAnonymous, "anonymous"),
        (SecurityIdentification, "identification"),
        (SecurityImpersonation, "impersonation"),
        (SecurityDelegation, "delegation"),
    ];
    names.iter().find(|(known, _)| *known == level).map_or_else(
        || format!("unknown level {level}"),
        |(_, name)| (*name).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::{require_impersonation, thread_level};
    use windows_sys::Win32::Security::{
        SecurityAnonymous, SecurityDelegation, SecurityIdentification, SecurityImpersonation,
    };

    #[test]
    fn levels_below_impersonation_are_refused_by_name() {
        for level in [SecurityAnonymous, SecurityIdentification] {
            let error = require_impersonation(level).err();
            let text = error.map(|e| e.to_string()).unwrap_or_default();
            assert!(
                text.contains("anonymous") || text.contains("identification"),
                "{text}"
            );
        }
        assert!(require_impersonation(SecurityImpersonation).is_ok());
        assert!(require_impersonation(SecurityDelegation).is_ok());
    }

    #[test]
    fn a_thread_without_a_token_is_an_error() {
        assert!(thread_level().is_err());
    }
}

#![allow(
    unsafe_code,
    reason = "interactive session identity requires Win32 FFI"
)]

use std::{io, ptr};

use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Security::LookupAccountNameW;
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SESSION, WTS_INFO_CLASS, WTSDomainName, WTSFreeMemory, WTSQuerySessionInformationW,
    WTSUserName,
};

use super::config::{Config, require_user_sid};
use super::elevation::{current_user_sid, sid_string, wide};

pub(super) fn installation_config() -> io::Result<Config> {
    let user = session_text(WTSUserName)?;
    let domain = session_text(WTSDomainName)?;
    if user.is_empty() {
        return Err(io::Error::other(
            "no interactive user in the installing session",
        ));
    }
    let sid = lookup_sid(&format!("{domain}\\{user}"))?;
    let search = if current_user_sid()? == sid {
        load_search_settings()?
    } else {
        tracing::warn!(
            "interactive user's settings require impersonation; using default service volumes"
        );
        rusty_store::Search::default()
    };
    Config::new(sid, search)
}

fn load_search_settings() -> io::Result<rusty_store::Search> {
    let Some(dirs) = rusty_store::StoreDirs::detect() else {
        return Ok(rusty_store::Search::default());
    };
    let (settings, report) = rusty_store::SettingsStore::load(&dirs.settings_file());
    if let Some(error) = report.error {
        return Err(io::Error::other(error));
    }
    Ok(settings.current().search.clone())
}

fn session_text(kind: WTS_INFO_CLASS) -> io::Result<String> {
    let mut raw = ptr::null_mut();
    let mut bytes = 0;
    // SAFETY: the query targets this process's local session and both outputs are writable.
    let ok = unsafe {
        WTSQuerySessionInformationW(
            ptr::null_mut(),
            WTS_CURRENT_SESSION,
            kind,
            &mut raw,
            &mut bytes,
        )
    };
    let _allocation = SessionAllocation(raw.cast());
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if raw.is_null() || bytes < 2 || bytes % 2 != 0 {
        return Err(io::Error::other("invalid interactive session identity"));
    }
    // SAFETY: WTS returned bytes initialized bytes in the UTF-16 allocation held above.
    let units = unsafe { std::slice::from_raw_parts(raw, bytes as usize / 2) };
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or_else(|| io::Error::other("unterminated interactive session identity"))?;
    String::from_utf16(&units[..end]).map_err(io::Error::other)
}

fn lookup_sid(account: &str) -> io::Result<String> {
    let account = wide(account);
    let (mut size, mut domain_size, mut kind) = (0, 0, 0);
    // SAFETY: NULL output buffers query their required lengths; the account is terminated.
    let ok = unsafe {
        LookupAccountNameW(
            ptr::null(),
            account.as_ptr(),
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            &mut domain_size,
            &mut kind,
        )
    };
    let error = io::Error::last_os_error();
    if ok == 0 && error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error);
    }
    if size == 0 {
        return Err(io::Error::other("interactive account has no SID"));
    }
    let mut sid = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    let mut domain = vec![0u16; domain_size as usize];
    // SAFETY: both writable buffers match the queried sizes; the account remains live.
    if unsafe {
        LookupAccountNameW(
            ptr::null(),
            account.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut kind,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    require_user_sid(kind)?;
    // SAFETY: the successful lookup initialized a SID in this aligned allocation.
    unsafe { sid_string(sid.as_mut_ptr().cast()) }
}

struct SessionAllocation(*mut std::ffi::c_void);

impl Drop for SessionAllocation {
    fn drop(&mut self) {
        // SAFETY: this guard owns the WTS allocation, or NULL.
        unsafe { WTSFreeMemory(self.0) };
    }
}

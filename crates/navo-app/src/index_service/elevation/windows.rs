#![allow(
    unsafe_code,
    reason = "elevation and token inspection require Win32 FFI"
)]

use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::{
    ERROR_CANCELLED, ERROR_INSUFFICIENT_BUFFER, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    IsUserAnAdmin, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::Action;
use super::elevation_result::{ElevationError, from_exit_code};

pub(super) fn is_admin() -> bool {
    // SAFETY: IsUserAnAdmin inspects the calling process token without pointer arguments.
    unsafe { IsUserAnAdmin() != 0 }
}

pub(super) fn launch(action: Action) -> Result<(), ElevationError> {
    let mode = match action {
        Action::Install => "install",
        Action::Remove => "remove",
        Action::Run => {
            return Err(io::Error::other("run is reserved for the service manager").into());
        }
    };
    let exe = std::env::current_exe()?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = wide("runas");
    let args = wide(&format!("--index-service {mode}"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: all input strings remain NUL-terminated and live until the synchronous launch returns.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            ElevationError::Cancelled
        } else {
            error.into()
        });
    }
    wait_for_child(info.hProcess)
}

fn wait_for_child(raw: *mut c_void) -> Result<(), ElevationError> {
    if raw.is_null() {
        return Err(io::Error::other("elevation returned no process handle").into());
    }
    // SAFETY: ShellExecuteExW transferred the process handle with NOCLOSEPROCESS.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: process remains live for this bounded wait.
    match unsafe { WaitForSingleObject(process.as_raw_handle(), 180_000) } {
        WAIT_OBJECT_0 => {}
        WAIT_TIMEOUT => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "elevated operation is still running",
            )
            .into());
        }
        _ => return Err(io::Error::last_os_error().into()),
    }
    let mut code = 0;
    // SAFETY: process is live and code is writable output storage.
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    from_exit_code(code)
}

pub(super) fn current_user_sid() -> io::Result<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: the process pseudo-handle is valid; raw is writable handle storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken succeeded and transferred a uniquely owned token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let user = token_user(&token)?;
    // SAFETY: token_user returns an aligned buffer containing TOKEN_USER and its SID.
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    // SAFETY: sid points into the live TOKEN_USER allocation.
    unsafe { sid_string(sid) }
}

pub(super) unsafe fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut text = ptr::null_mut();
    // SAFETY: the caller supplies a live SID; text receives a LocalAlloc allocation.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let allocation = LocalAllocation(text.cast());
    // SAFETY: the API returned a valid NUL-terminated UTF-16 SID allocation.
    let result = unsafe {
        let length = (0..184)
            .find(|&at| *text.add(at) == 0)
            .ok_or_else(|| io::Error::other("invalid token SID length"))?;
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    };
    drop(allocation);
    Ok(result)
}

fn token_user(token: &OwnedHandle) -> io::Result<Vec<usize>> {
    let mut length = 0;
    // SAFETY: a NULL output with zero length queries the required buffer size.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut length,
        )
    };
    let error = io::Error::last_os_error();
    if ok == 0 && error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error);
    }
    if (length as usize) < size_of::<TOKEN_USER>() {
        return Err(io::Error::other("missing token user"));
    }
    let mut user = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: the aligned allocation contains at least length writable bytes; the token remains live.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(user)
}

pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) struct LocalAllocation(pub *mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the LocalAlloc allocation, or NULL.
        unsafe { LocalFree(self.0) };
    }
}

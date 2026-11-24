//! IW25 launcher: starts Setup's elevated folder helper through the UAC prompt and waits for its
//! exit code. Runs in the non-elevated wizard; the helper's side is `elevate_entry`.

#![allow(
    unsafe_code,
    reason = "ShellExecuteExW and process waits are Win32 FFI"
)]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{ERROR_CANCELLED, MAX_PATH, WAIT_OBJECT_0};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::elevate::{HelperArgs, launch_parameters};

/// Why the helper did not run to an exit code.
#[derive(Debug)]
pub enum LaunchError {
    /// The user answered No at the UAC prompt (`ERROR_CANCELLED`).
    Declined,
    /// Refused arguments, a failed launch, wait or exit-code read.
    Io(io::Error),
}

impl From<io::Error> for LaunchError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Starts `current_exe() --elevated-create …` with the `runas` verb, waits with no deadline and
/// returns the helper's raw exit code. Refused arguments fail before any prompt.
pub fn launch_helper(args: &HelperArgs) -> Result<u32, LaunchError> {
    let parameters = launch_parameters(args)?;
    let exe = wide_path(&std::env::current_exe()?)?;
    let directory = system_directory()?;
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: directory.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: every string is NUL-terminated and outlives this synchronous (NOASYNC) call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Err(LaunchError::Declined);
        }
        return Err(error.into());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("the elevated start returned no process handle").into());
    }
    // SAFETY: NOCLOSEPROCESS hands the caller a process handle it must close; OwnedHandle does.
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    wait_exit_code(&process).map_err(LaunchError::Io)
}

/// Waits for the process to end (no deadline: the user may sit at the prompt) and reads its code.
fn wait_exit_code(process: &OwnedHandle) -> io::Result<u32> {
    // SAFETY: the handle is live and owned for the duration of the wait.
    if unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let mut code = 0;
    // SAFETY: the handle is live; `code` is writable storage for one u32.
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

/// The path as NUL-terminated UTF-16; `InvalidInput` when it holds a NUL.
fn wide_path(path: &std::path::Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path holds a NUL",
        ));
    }
    wide.push(0);
    Ok(wide)
}

/// The System32 folder as NUL-terminated UTF-16, the helper's working folder.
fn system_directory() -> io::Result<Vec<u16>> {
    let mut buffer = vec![0u16; MAX_PATH as usize];
    // SAFETY: the buffer is writable for the length passed.
    let len = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    if len >= buffer.len() {
        return Err(io::Error::other(
            "the System32 path is longer than MAX_PATH",
        ));
    }
    buffer.truncate(len + 1);
    Ok(buffer)
}

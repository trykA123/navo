use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Services::{
    QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatusEx, SC_STATUS_PROCESS_INFO,
    SERVICE_STATUS_PROCESS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};

use super::super::install_check::{Expected, Process, Running, check_image, check_running};
use super::super::install_plan::CopyPlan;
use super::super::scm::{ServiceHandle, checked};

const CONFIG_BYTES: usize = 8 * 1024;
const PATH_UNITS: usize = 32_768;

/// The current time as FILETIME ticks; a service process created later runs the new image.
pub(super) fn now() -> u64 {
    let mut time = FILETIME::default();
    // SAFETY: time is writable output storage.
    unsafe { GetSystemTimeAsFileTime(&mut time) };
    ticks(time)
}

/// Fails unless the image equals this exe, the service is registered with `command`, and
/// its process was started from the image after `started_after`.
pub(super) fn installed(
    service: &ServiceHandle,
    plan: &CopyPlan,
    command: &[u16],
    started_after: Option<u64>,
) -> io::Result<()> {
    let started_after =
        started_after.ok_or_else(|| io::Error::other("the install check ran before the start"))?;
    check_image(&std::env::current_exe()?, &plan.executable)?;
    let image = std::fs::canonicalize(&plan.executable)?;
    let command = OsString::from_wide(command.strip_suffix(&[0]).unwrap_or(command));
    let registered = registered_command(service)?;
    let expected = Expected {
        command: &command,
        image: &image,
        started_after,
    };
    let running = Running {
        command: &registered,
        process: service_process(service)?,
    };
    check_running(&expected, &running).map_err(io::Error::other)
}

fn registered_command(service: &ServiceHandle) -> io::Result<OsString> {
    let mut buffer = vec![0usize; CONFIG_BYTES / size_of::<usize>()];
    let mut needed = 0;
    // SAFETY: buffer is aligned writable storage of CONFIG_BYTES, the documented maximum configuration size.
    checked(unsafe {
        QueryServiceConfigW(
            service.0,
            buffer.as_mut_ptr().cast(),
            CONFIG_BYTES as u32,
            &mut needed,
        )
    })?;
    // SAFETY: QueryServiceConfigW initialized the aligned buffer with a QUERY_SERVICE_CONFIGW.
    let config = unsafe { &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
    if config.lpBinaryPathName.is_null() {
        return Ok(OsString::new());
    }
    // SAFETY: the binary path is a NUL-terminated string inside the live buffer.
    let units = unsafe {
        let length = (0..CONFIG_BYTES / 2)
            .find(|&at| *config.lpBinaryPathName.add(at) == 0)
            .ok_or_else(|| io::Error::other("unterminated service binary path"))?;
        std::slice::from_raw_parts(config.lpBinaryPathName, length)
    };
    Ok(OsString::from_wide(units))
}

fn service_process(service: &ServiceHandle) -> io::Result<Option<Process>> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    // SAFETY: status is writable storage of the size passed; the handle has QUERY_STATUS access.
    checked(unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            (&raw mut status).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    })?;
    if status.dwProcessId == 0 {
        return Ok(None);
    }
    // SAFETY: requests only limited query access for a process ID, with no inherited handle.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, status.dwProcessId) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcess returned a new handle that only this guard closes.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    Ok(Some(Process {
        image: std::fs::canonicalize(process_image(&process)?)?,
        created: created(&process)?,
    }))
}

fn process_image(process: &OwnedHandle) -> io::Result<PathBuf> {
    let mut path = vec![0u16; PATH_UNITS];
    let mut length = PATH_UNITS as u32;
    // SAFETY: the process handle is live and `path` has `length` writable UTF-16 units.
    checked(unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle(),
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &raw mut length,
        )
    })?;
    let units = path
        .get(..length as usize)
        .ok_or_else(|| io::Error::other("process image path too long"))?;
    Ok(PathBuf::from(OsString::from_wide(units)))
}

fn created(process: &OwnedHandle) -> io::Result<u64> {
    let [mut creation, mut exit, mut kernel, mut user] = [FILETIME::default(); 4];
    // SAFETY: the process handle is live and all four outputs are writable.
    checked(unsafe {
        GetProcessTimes(
            process.as_raw_handle(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    })?;
    Ok(ticks(creation))
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

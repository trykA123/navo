use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use navo_index::pipe::Client;
use windows_sys::Win32::Foundation::{
    ERROR_SERVICE_DOES_NOT_EXIST, FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Services::{
    OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatusEx,
    SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_STATUS_PROCESS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles;

use super::super::install_check::{
    Expected, Mismatch, Process, Running, Seen, check_answer, check_image, check_running,
    is_mismatch,
};
use super::super::install_plan::{CopyPlan, NAVO};
use super::super::scm::{ServiceHandle, checked, open_manager, service_handle, service_name};
use super::super::upgrade::await_answer;

const CONFIG_BYTES: usize = 8 * 1024;
const PATH_UNITS: usize = 32_768;
const ANSWER_TRY: Duration = Duration::from_secs(2);

/// Checks the current service without creating files or changing its registration or state.
pub(super) fn current(source: &Path) -> io::Result<bool> {
    if super::super::scm::query_running(NAVO)? != Some(true) {
        return Ok(false);
    }
    let manager = open_manager(SC_MANAGER_CONNECT)?;
    let name = service_name();
    // SAFETY: manager and terminated name remain live; only read-only access is requested.
    let raw = unsafe {
        OpenServiceW(
            manager.0,
            name.as_ptr(),
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        )
    };
    let service = match service_handle(raw) {
        Ok(service) => service,
        Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    let plan = CopyPlan::new(
        &super::super::directory::known_folder(&FOLDERID_ProgramFiles)?,
        std::process::id(),
    );
    let checked = installed(
        &service,
        (&plan, source),
        &super::binary_command(&plan),
        Some(0),
    )
    .and_then(|bound| answer(&service, &bound, (Duration::ZERO, Duration::ZERO)));
    match checked {
        Ok(()) => Ok(true),
        Err(error)
            if is_mismatch(&error)
                || matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::TimedOut
                ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// The verified service process, opened by Verify and held until the commit point.
pub(super) struct Bound {
    id: u32,
    process: OwnedHandle,
}

/// The current time as FILETIME ticks; a service process created later runs the new image.
pub(super) fn now() -> u64 {
    let mut time = FILETIME::default();
    // SAFETY: time is writable output storage.
    unsafe { GetSystemTimeAsFileTime(&mut time) };
    ticks(time)
}

/// Fails unless the image equals `source`, the service is registered with `command`, and
/// its process was started from the image after `started_after`; returns that process, held.
pub(super) fn installed(
    service: &ServiceHandle,
    (plan, source): (&CopyPlan, &Path),
    command: &[u16],
    started_after: Option<u64>,
) -> io::Result<Bound> {
    let started_after =
        started_after.ok_or_else(|| io::Error::other("the install check ran before the start"))?;
    check_image(source, &plan.executable)?;
    let image = std::fs::canonicalize(&plan.executable)?;
    let command = OsString::from_wide(command.strip_suffix(&[0]).unwrap_or(command));
    let registered = registered_command(service)?;
    let expected = Expected {
        command: &command,
        image: &image,
        started_after,
    };
    let id = service_process_id(service)?;
    let opened = (id != 0).then(|| open_process(id)).transpose()?;
    let running = Running {
        command: &registered,
        process: opened.as_ref().map(process_facts).transpose()?,
    };
    check_running(&expected, &running).map_err(io::Error::other)?;
    let process = opened.ok_or_else(|| io::Error::other(Mismatch::NotRunning))?;
    Ok(Bound { id, process })
}

/// Polls the pipe until the verified process answers the current revision; see `check_answer`.
pub(super) fn answer(
    service: &ServiceHandle,
    bound: &Bound,
    wait: (Duration, Duration),
) -> io::Result<()> {
    await_answer(wait, || {
        let reply = Client::version_from_server(Instant::now() + ANSWER_TRY);
        let seen = Seen {
            reply: reply
                .as_ref()
                .ok()
                .map(|(version, server)| (version, *server)),
            alive: is_alive(&bound.process)?,
            scm: service_process_id(service)?,
        };
        if check_answer(bound.id, &seen).map_err(io::Error::other)? {
            return Ok(());
        }
        Err(reply.err().map_or_else(
            || io::Error::other("the service pipe gave no answer"),
            io::Error::other,
        ))
    })
}

fn is_alive(process: &OwnedHandle) -> io::Result<bool> {
    // SAFETY: the process handle is live and was opened with SYNCHRONIZE access.
    match unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } {
        WAIT_TIMEOUT => Ok(true),
        WAIT_OBJECT_0 => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
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

fn service_process_id(service: &ServiceHandle) -> io::Result<u32> {
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
    Ok(status.dwProcessId)
}

fn open_process(id: u32) -> io::Result<OwnedHandle> {
    let access = PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
    // SAFETY: requests only wait and limited query access for a process ID, with no inherited handle.
    let raw = unsafe { OpenProcess(access, 0, id) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcess returned a new handle that only this guard closes.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn process_facts(process: &OwnedHandle) -> io::Result<Process> {
    Ok(Process {
        image: std::fs::canonicalize(process_image(process)?)?,
        created: created(process)?,
    })
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

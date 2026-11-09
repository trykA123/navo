//! Windows SCM implementation of the portable stop and start control steps.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_CANNOT_ACCEPT_CTRL, ERROR_SERVICE_NOT_ACTIVE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Services::{
    ControlService, QueryServiceStatusEx, SC_STATUS_PROCESS_INFO, SERVICE_CONTROL_STOP,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, StartServiceW,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use super::super::removal::{Control, POLL, StartResult, State, Status, StopLimits};
use super::super::scm::{ServiceHandle, checked};

/// Stops the service and waits for its process to exit; needs STOP and QUERY_STATUS access.
pub(super) fn stop(service: &ServiceHandle, limits: StopLimits) -> io::Result<()> {
    let mut control = ScmControl {
        service,
        process: None,
    };
    super::super::removal::stop(&mut control, limits)
}

/// Starts the service and waits until RUNNING; needs START and QUERY_STATUS access.
pub(super) fn start(service: &ServiceHandle) -> io::Result<()> {
    let mut control = ScmControl {
        service,
        process: None,
    };
    super::super::removal::start(&mut control, StopLimits::INSTALL)
}

struct ScmControl<'a> {
    service: &'a ServiceHandle,
    process: Option<OwnedHandle>,
}

impl Control for ScmControl<'_> {
    fn status(&mut self) -> io::Result<Status> {
        let mut status = SERVICE_STATUS_PROCESS::default();
        let mut needed = 0;
        // SAFETY: status is writable storage of the size passed; the handle has QUERY_STATUS access.
        checked(unsafe {
            QueryServiceStatusEx(
                self.service.0,
                SC_STATUS_PROCESS_INFO,
                (&raw mut status).cast(),
                size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        })?;
        Ok(Status {
            state: State::from_raw(status.dwCurrentState)?,
            process: status.dwProcessId,
        })
    }

    fn open(&mut self, process: u32) -> io::Result<()> {
        // SAFETY: requests only wait access for a process ID, with no inherited handle.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: OpenProcess returned a new handle that only this guard closes.
        self.process = Some(unsafe { OwnedHandle::from_raw_handle(raw) });
        Ok(())
    }

    fn release(&mut self) {
        self.process = None;
    }

    fn stop(&mut self) -> io::Result<bool> {
        let mut status = SERVICE_STATUS::default();
        // SAFETY: the handle has STOP access and status is writable output.
        if unsafe { ControlService(self.service.0, SERVICE_CONTROL_STOP, &mut status) } != 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_SERVICE_NOT_ACTIVE) => Ok(true),
            Some(ERROR_SERVICE_CANNOT_ACCEPT_CTRL) => Ok(false),
            _ => Err(error),
        }
    }

    fn start(&mut self) -> io::Result<StartResult> {
        // SAFETY: the handle has START access and no arguments are passed.
        match checked(unsafe { StartServiceW(self.service.0, 0, ptr::null()) }) {
            Ok(()) => Ok(StartResult::Started),
            Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_ALREADY_RUNNING as i32) => {
                Ok(StartResult::AlreadyRunning)
            }
            Err(error) => Err(error),
        }
    }

    fn wait(&mut self, limit: Duration) -> io::Result<bool> {
        let Some(process) = &self.process else {
            return Ok(true);
        };
        // u32::MAX is INFINITE; the limit is seconds, so it never gets there.
        let millis = u32::try_from(limit.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: the process handle is live and was opened with SYNCHRONIZE access.
        match unsafe { WaitForSingleObject(process.as_raw_handle(), millis) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn pause(&mut self) {
        std::thread::sleep(POLL);
    }
}

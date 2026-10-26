#![allow(
    unsafe_code,
    reason = "service installation and dispatch require Win32 FFI"
)]

use std::ffi::c_void;
use std::io;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::{panic, ptr};

use rusty_index::pipe::CancelToken;
use windows_sys::Win32::Foundation::{
    ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_SPECIFIC_ERROR,
};
use windows_sys::Win32::System::Services::*;

use super::install_plan::{NAVO, Product};
use super::policy::ServiceError;

static CANCEL: OnceLock<CancelToken> = OnceLock::new();
static STATUS: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static STATE: AtomicU32 = AtomicU32::new(SERVICE_START_PENDING);
static CHECKPOINT: AtomicU32 = AtomicU32::new(0);

pub(super) struct ServiceHandle(pub SC_HANDLE);

pub(super) fn query_running(product: Product) -> io::Result<Option<bool>> {
    let manager = open_manager(SC_MANAGER_CONNECT)?;
    let name = service_name_of(product);
    // SAFETY: manager and terminated name remain live; only read-only status access is requested.
    let raw = unsafe { OpenServiceW(manager.0, name.as_ptr(), SERVICE_QUERY_STATUS) };
    let service = match service_handle(raw) {
        Ok(service) => service,
        Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let mut status = SERVICE_STATUS::default();
    // SAFETY: service has QUERY_STATUS access; status is writable output storage.
    checked(unsafe { QueryServiceStatus(service.0, &mut status) })?;
    Ok(Some(status.dwCurrentState == SERVICE_RUNNING))
}

pub(super) fn dispatch() -> io::Result<()> {
    let mut name = service_name();
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the terminated dispatch table and its name remain live until the dispatcher returns.
    checked(unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) })
}

unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let code = match panic::catch_unwind(start_service) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => match error.stop_code() {
            Some(code) => code,
            None => std::process::exit(1),
        },
        Err(_) => std::process::exit(1),
    };
    if report(SERVICE_STOPPED, code).is_err() {
        std::process::exit(1);
    }
}

fn start_service() -> Result<(), ServiceError> {
    let name = service_name();
    CANCEL.get_or_init(CancelToken::new);
    // SAFETY: handler has the Win32 callback signature; no context is dereferenced and name lives through registration.
    let status =
        unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control), ptr::null()) };
    if status.is_null() {
        return Err(ServiceError::Unexpected(io::Error::last_os_error()));
    }
    STATUS.store(status, Ordering::Release);
    report(SERVICE_START_PENDING, 0).map_err(ServiceError::Unexpected)?;
    super::runtime::run(CANCEL.get_or_init(CancelToken::new), || {
        report(SERVICE_RUNNING, 0)
    })
}

unsafe extern "system" fn control(code: u32, _: u32, _: *mut c_void, _: *mut c_void) -> u32 {
    let result = match code {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            CANCEL.get_or_init(CancelToken::new).cancel();
            report(SERVICE_STOP_PENDING, 0)
        }
        SERVICE_CONTROL_INTERROGATE => report(STATE.load(Ordering::Acquire), 0),
        _ => return ERROR_CALL_NOT_IMPLEMENTED,
    };
    result
        .err()
        .and_then(|error| error.raw_os_error())
        .map_or(0, |code| code as u32)
}

fn report(state: u32, code: u32) -> io::Result<()> {
    let pending = matches!(state, SERVICE_START_PENDING | SERVICE_STOP_PENDING);
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: if code != 0 {
            ERROR_SERVICE_SPECIFIC_ERROR
        } else {
            0
        },
        dwServiceSpecificExitCode: code,
        dwCheckPoint: if pending {
            CHECKPOINT.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            CHECKPOINT.store(0, Ordering::Relaxed);
            0
        },
        dwWaitHint: if pending { 30000 } else { 0 },
    };
    STATE.store(state, Ordering::Release);
    // SAFETY: STATUS holds the registered SCM status handle; status is initialized and lives through the call.
    checked(unsafe { SetServiceStatus(STATUS.load(Ordering::Acquire), &status) })
}

pub(super) fn stopping() -> io::Result<()> {
    report(SERVICE_STOP_PENDING, 0)
}

pub(super) fn service_name() -> Vec<u16> {
    service_name_of(NAVO)
}

pub(super) fn service_name_of(product: Product) -> Vec<u16> {
    product
        .service
        .iter()
        .copied()
        .map(u16::from)
        .chain([0])
        .collect()
}

pub(super) fn open_manager(access: u32) -> io::Result<ServiceHandle> {
    // SAFETY: NULL machine and database select the local service manager.
    service_handle(unsafe { OpenSCManagerW(ptr::null(), ptr::null(), access) })
}

pub(super) fn service_handle(raw: SC_HANDLE) -> io::Result<ServiceHandle> {
    if raw.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(ServiceHandle(raw))
    }
}

pub(super) fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the SCM handle and closes it exactly once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

#[path = "windows_pipe.rs"]
mod pipe;
#[path = "windows_security.rs"]
mod security;

use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, GetLastError, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

use super::server::Server;
use super::{Acquired, Error, Reply, Request, protocol};

pub(crate) struct Guard {
    server: Server,
    pipe: Option<pipe::Pipe>,
    _mutex: OwnedHandle,
}

pub(crate) struct Client {
    name: Vec<u16>,
}

pub(super) fn acquire(hash: &str, _: &Path) -> Result<Acquired, Error> {
    acquire_with(hash, pipe::Pipe::create)
}

fn acquire_with(
    hash: &str,
    create: impl FnOnce(&[u16]) -> io::Result<pipe::Pipe>,
) -> Result<Acquired, Error> {
    let mutex_name = wide(&format!("Local\\Ferrite-{hash}"));
    // SAFETY: the name is NUL-terminated; default security and no initial ownership require no other pointers.
    let raw = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    // SAFETY: GetLastError reads this thread's error immediately after CreateMutexW.
    let error = unsafe { GetLastError() };
    let mutex = owned(raw)?;
    let name = pipe_name(hash);
    if error == ERROR_ALREADY_EXISTS {
        return Ok(Acquired::Second(Client {
            name: name.map_err(|_| Error::Timeout)?,
        }));
    }
    let pipe = match name.and_then(|name| create(&name)) {
        Ok(pipe) => Some(pipe),
        Err(error) => {
            tracing::warn!(%error, "cannot create instance pipe; starting without instance server");
            None
        }
    };
    Ok(Acquired::First(Guard {
        server: Server::default(),
        pipe,
        _mutex: mutex,
    }))
}

impl Guard {
    pub(crate) fn serve(
        &mut self,
        handler: impl Fn(Request) -> Reply + Send + 'static,
    ) -> Result<(), Error> {
        let Some(mut pipe) = self.pipe.take() else {
            return Ok(());
        };
        if let Err(error) = self
            .server
            .start(move |stop| serve_connections(&mut pipe, &handler, &stop))
        {
            tracing::warn!(%error, "cannot start instance server; keeping instance mutex");
        }
        Ok(())
    }
}

fn serve_connections(
    pipe: &mut pipe::Pipe,
    handler: &impl Fn(Request) -> Reply,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        match pipe.connect() {
            Ok(true) => {
                protocol::serve(pipe, handler, stop);
                pipe.finish(stop);
            }
            Ok(false) => std::thread::park_timeout(protocol::IDLE_POLL),
            Err(error) => {
                tracing::error!(%error, "instance pipe failed");
                break;
            }
        }
    }
}

impl Client {
    pub(crate) fn send(&self, request: &Request, timeout: Duration) -> Result<Reply, Error> {
        let deadline = Instant::now() + timeout;
        // SAFETY: this permits foreground activation; ASFW_ANY is the documented process-id sentinel.
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        let mut pipe = loop {
            match pipe::Pipe::open(&self.name) {
                Ok(pipe) => break pipe,
                Err(error) if matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PIPE_BUSY as i32) =>
                {
                    protocol::wait_for_connection(deadline)?;
                }
                Err(error) => return Err(error.into()),
            }
        };
        pipe.set_nonblocking().map_err(|_| Error::Timeout)?;
        protocol::exchange(&mut pipe, request, deadline)
    }
}

fn wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(Some(0)).collect()
}

fn pipe_name(hash: &str) -> io::Result<Vec<u16>> {
    let mut session = 0;
    // SAFETY: the process id is current and session points to writable u32 storage.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(wide(&format!("\\\\.\\pipe\\Ferrite-{session}-{hash}")))
}

#[cfg(test)]
pub(super) fn connect_and_close(profile: &Path) {
    let hash = super::naming::profile_hash(profile).unwrap();
    let name = pipe_name(&hash).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match pipe::Pipe::open(&name) {
            Ok(pipe) => {
                drop(pipe);
                return;
            }
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                protocol::wait_for_connection(deadline).unwrap();
            }
            Err(error) => panic!("cannot open the pipe: {error}"),
        }
    }
}

fn owned(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the API returned a valid, uniquely owned kernel handle; OwnedHandle closes it exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;

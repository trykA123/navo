#[path = "windows_pipe.rs"]
mod pipe;

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
use windows_sys::Win32::System::Threading::CreateMutexW;
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
    let mutex_name = wide(&format!("Local\\Ferrite-{hash}"));
    // SAFETY: the name is NUL-terminated; default security and no initial ownership require no other pointers.
    let raw = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    // SAFETY: GetLastError reads this thread's error immediately after CreateMutexW.
    let error = unsafe { GetLastError() };
    let mutex = owned(raw)?;
    let name = wide(&format!("\\\\.\\pipe\\Ferrite-{hash}"));
    if error == ERROR_ALREADY_EXISTS {
        return Ok(Acquired::Second(Client { name }));
    }
    let pipe = pipe::Pipe::create(&name)?;
    Ok(Acquired::First(Guard {
        server: Server::default(),
        pipe: Some(pipe),
        _mutex: mutex,
    }))
}

impl Guard {
    pub(crate) fn serve(
        &mut self,
        handler: impl Fn(Request) -> Reply + Send + 'static,
    ) -> Result<(), Error> {
        let mut pipe = self.pipe.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "instance server already started",
            )
        })?;
        self.server
            .start(move |stop| serve_connections(&mut pipe, &handler, &stop))
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
            Ok(false) => std::thread::park_timeout(protocol::POLL),
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
                    protocol::pause(deadline, &AtomicBool::new(false))?;
                }
                Err(error) => return Err(error.into()),
            }
        };
        protocol::exchange(&mut pipe, request, deadline)
    }
}

fn wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(Some(0)).collect()
}

fn owned(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the API returned a valid, uniquely owned kernel handle; OwnedHandle closes it exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

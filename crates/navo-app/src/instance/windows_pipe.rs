use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_NOT_FOUND, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING,
    GENERIC_READ, GENERIC_WRITE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, SetNamedPipeHandleState,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, WaitForMultipleObjects, WaitForSingleObject,
};

use super::super::server::{Completion, classify_completion, wait_for_pending};
use super::{owned, security::PipeSecurity};

pub(super) struct Pipe {
    handle: OwnedHandle,
    event: Option<OwnedHandle>,
}

pub(super) struct Connection<'a> {
    pipe: &'a mut Pipe,
    stop: &'a OwnedHandle,
    deadline: Instant,
    ended: bool,
}

impl Pipe {
    pub(super) fn create(name: &[u16]) -> io::Result<Self> {
        let security = PipeSecurity::current_user()?;
        let attributes = security.attributes();
        let event = event()?;
        // SAFETY: name is NUL-terminated, attributes and its descriptor live through the call, and the byte-pipe flags are valid.
        let raw = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                &attributes,
            )
        };
        Ok(Self {
            handle: owned(raw)?,
            event: Some(event),
        })
    }

    pub(super) fn open(name: &[u16]) -> io::Result<Self> {
        // SAFETY: name is a NUL-terminated pipe path; the call uses default security and no template handle.
        let raw = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        let handle = owned(raw)?;
        let mode = PIPE_NOWAIT;
        // SAFETY: only this synchronous client handle is changed; mode is valid and optional settings are null.
        if unsafe { SetNamedPipeHandleState(handle.as_raw_handle(), &mode, null(), null()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle,
            event: None,
        })
    }

    pub(super) fn connect(&mut self, stop: &OwnedHandle) -> io::Result<bool> {
        loop {
            // SAFETY: stop is a live event handle owned by the caller.
            if unsafe { WaitForSingleObject(stop.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            match self.connect_once(stop) {
                Err(error) if error.raw_os_error() == Some(ERROR_NO_DATA as i32) => {
                    self.disconnect()?;
                }
                result => return result,
            }
        }
    }

    fn connect_once(&self, stop: &OwnedHandle) -> io::Result<bool> {
        let mut request = self.overlapped()?;
        let pending = &raw mut request;
        // SAFETY: pending and its event stay live until completion is drained on every path.
        if unsafe { ConnectNamedPipe(self.handle.as_raw_handle(), pending) } != 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED as i32 => Ok(true),
            Some(code) if code == ERROR_IO_PENDING as i32 => {
                self.wait_pending(pending, stop, INFINITE)?;
                Ok(true)
            }
            _ => Err(error),
        }
    }

    pub(super) fn connection<'a>(
        &'a mut self,
        stop: &'a OwnedHandle,
        deadline: Instant,
    ) -> Connection<'a> {
        Connection {
            pipe: self,
            stop,
            deadline,
            ended: false,
        }
    }

    fn overlapped(&self) -> io::Result<OVERLAPPED> {
        let event = self
            .event
            .as_ref()
            .ok_or_else(|| io::Error::other("instance server event missing"))?;
        // SAFETY: the event is live, and no previous operation remains pending.
        if unsafe { ResetEvent(event.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(OVERLAPPED {
            hEvent: event.as_raw_handle(),
            ..OVERLAPPED::default()
        })
    }

    fn wait_pending(
        &self,
        pending: *const OVERLAPPED,
        stop: &OwnedHandle,
        timeout: u32,
    ) -> io::Result<usize> {
        let event = self
            .event
            .as_ref()
            .ok_or_else(|| io::Error::other("instance server event missing"))?;
        let events = [stop.as_raw_handle(), event.as_raw_handle()];
        let mut count = 0;
        wait_for_pending(
            || {
                // SAFETY: both events stay live until the pending operation is drained.
                match unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, timeout) } {
                    WAIT_OBJECT_0 => Err(io::ErrorKind::ConnectionAborted.into()),
                    code if code == WAIT_OBJECT_0 + 1 => Ok(true),
                    WAIT_TIMEOUT => Err(io::ErrorKind::TimedOut.into()),
                    _ => Err(io::Error::last_os_error()),
                }
            },
            || self.cancel(pending),
            || {
                count = self.complete(pending)?;
                Ok(())
            },
        )?;
        Ok(count)
    }

    fn cancel(&self, pending: *const OVERLAPPED) {
        // SAFETY: pending identifies an operation on this live handle; its storage is not released here.
        if unsafe { CancelIoEx(self.handle.as_raw_handle(), pending) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
                tracing::warn!(%error, "cannot cancel instance pipe operation");
            }
        }
    }

    fn complete(&self, pending: *const OVERLAPPED) -> io::Result<usize> {
        let mut count = 0;
        // SAFETY: pending and any I/O buffer remain live until this wait drains the operation.
        let ok =
            unsafe { GetOverlappedResult(self.handle.as_raw_handle(), pending, &mut count, 1) };
        completion(ok, count)
    }

    fn transfer(
        &self,
        start: impl FnOnce(*mut OVERLAPPED, *mut u32) -> i32,
        connection: Option<&Connection<'_>>,
    ) -> io::Result<usize> {
        let mut request = match connection {
            Some(_) => self.overlapped()?,
            None => OVERLAPPED::default(),
        };
        let pending = if connection.is_some() {
            &raw mut request
        } else {
            null_mut()
        };
        let mut count = 0;
        let ok = start(pending, &raw mut count);
        if ok != 0 {
            return completion(ok, count);
        }
        let error = io::Error::last_os_error();
        if let Some(connection) = connection {
            if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
                let remaining = connection
                    .deadline
                    .saturating_duration_since(Instant::now());
                let timeout = u32::try_from(remaining.as_millis())
                    .unwrap_or(INFINITE - 1)
                    .min(INFINITE - 1);
                return self.wait_pending(pending, connection.stop, timeout);
            }
            return classify(error.raw_os_error().unwrap_or(1) as u32, count);
        }
        match error.raw_os_error() {
            Some(code) if code == ERROR_NO_DATA as i32 || code == ERROR_PIPE_LISTENING as i32 => {
                Err(io::ErrorKind::WouldBlock.into())
            }
            _ => Err(error),
        }
    }

    fn read_buffer(
        &self,
        buffer: &mut [u8],
        connection: Option<&Connection<'_>>,
    ) -> io::Result<usize> {
        let length = buffer.len().min(u32::MAX as usize) as u32;
        self.transfer(
            |pending, count| {
                // SAFETY: buffer, count and request remain live until transfer drains pending I/O.
                unsafe {
                    ReadFile(
                        self.handle.as_raw_handle(),
                        buffer.as_mut_ptr(),
                        length,
                        count,
                        pending,
                    )
                }
            },
            connection,
        )
    }

    fn write_buffer(
        &self,
        buffer: &[u8],
        connection: Option<&Connection<'_>>,
    ) -> io::Result<usize> {
        let length = buffer.len().min(u32::MAX as usize) as u32;
        let count = self.transfer(
            |pending, count| {
                // SAFETY: buffer, count and request remain live until transfer drains pending I/O.
                unsafe {
                    WriteFile(
                        self.handle.as_raw_handle(),
                        buffer.as_ptr(),
                        length,
                        count,
                        pending,
                    )
                }
            },
            connection,
        )?;
        if count == 0 {
            return Err(if connection.is_some() {
                io::ErrorKind::BrokenPipe
            } else {
                io::ErrorKind::WouldBlock
            }
            .into());
        }
        Ok(count)
    }

    fn disconnect(&mut self) -> io::Result<()> {
        // SAFETY: this thread exclusively owns the live server pipe and no I/O is pending.
        if unsafe { DisconnectNamedPipe(self.handle.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Connection<'_> {
    pub(super) fn finish(mut self) {
        if !self.ended
            && let Err(error) = self.read(&mut [0])
        {
            tracing::debug!(%error, "instance peer close wait ended");
        }
        if let Err(error) = self.pipe.disconnect() {
            tracing::debug!(%error, "instance pipe disconnected");
        }
    }
}

impl Read for Connection<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.ended {
            return Ok(0);
        }
        let result = self
            .pipe
            .read_buffer(buffer, Some(self))
            .map_err(io::Error::other);
        self.ended = !matches!(result, Ok(count) if count > 0);
        result
    }
}

impl Write for Connection<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.ended {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let result = self
            .pipe
            .write_buffer(buffer, Some(self))
            .map_err(io::Error::other);
        self.ended = result.is_err();
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for Pipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.read_buffer(buffer, None)
    }
}

impl Write for Pipe {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.write_buffer(buffer, None)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn event() -> io::Result<OwnedHandle> {
    // SAFETY: creates an unnamed manual-reset event with default security and no initial signal.
    owned(unsafe { CreateEventW(null(), 1, 0, null()) })
}

fn completion(ok: i32, count: u32) -> io::Result<usize> {
    let code = if ok == 0 {
        // SAFETY: GetLastError reads this thread's error immediately after the failed I/O call.
        unsafe { windows_sys::Win32::Foundation::GetLastError() }
    } else {
        0
    };
    classify(code, count)
}

fn classify(code: u32, count: u32) -> io::Result<usize> {
    match classify_completion(code, count) {
        Completion::Gone => Ok(0),
        Completion::Done(count) => Ok(count as usize),
        Completion::Fatal(code) => Err(io::Error::from_raw_os_error(code as i32)),
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;

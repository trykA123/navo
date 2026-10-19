use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    ERROR_NO_DATA, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, GENERIC_READ, GENERIC_WRITE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
    WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PeekNamedPipe, SetNamedPipeHandleState,
};

use super::super::protocol;
use super::{owned, security::PipeSecurity};

pub(super) struct Pipe(OwnedHandle);

impl Pipe {
    pub(super) fn create(name: &[u16]) -> io::Result<Self> {
        let security = PipeSecurity::current_user()?;
        let attributes = security.attributes();
        // SAFETY: name is NUL-terminated, attributes and its descriptor live through the call, and the byte-pipe flags are valid.
        let raw = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                &attributes,
            )
        };
        owned(raw).map(Self)
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
        owned(raw).map(Self)
    }

    pub(super) fn set_nonblocking(&self) -> io::Result<()> {
        let mode = PIPE_NOWAIT;
        // SAFETY: the pipe handle is live and mode points to a valid wait-mode flag; other settings are unchanged.
        let ok = unsafe { SetNamedPipeHandleState(self.0.as_raw_handle(), &mode, null(), null()) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn connect(&mut self) -> io::Result<bool> {
        // SAFETY: this live pipe uses nonblocking synchronous mode, so no OVERLAPPED pointer is needed.
        let ok = unsafe { ConnectNamedPipe(self.0.as_raw_handle(), null_mut()) };
        if ok != 0 {
            return Ok(false);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED as i32 => Ok(true),
            Some(code) if code == ERROR_PIPE_LISTENING as i32 => Ok(false),
            Some(code) if code == ERROR_NO_DATA as i32 => {
                self.disconnect();
                Ok(false)
            }
            _ => Err(error),
        }
    }

    pub(super) fn finish(&mut self, stop: &AtomicBool) {
        let deadline = Instant::now() + protocol::TIMEOUT;
        loop {
            // SAFETY: the live pipe is only inspected; optional output pointers are all null.
            let connected = unsafe {
                PeekNamedPipe(
                    self.0.as_raw_handle(),
                    null_mut(),
                    0,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if connected == 0 || protocol::pause(deadline, stop).is_err() {
                break;
            }
        }
        self.disconnect();
    }

    fn disconnect(&mut self) {
        // SAFETY: this thread exclusively owns the live server pipe and no I/O is pending.
        let ok = unsafe { DisconnectNamedPipe(self.0.as_raw_handle()) };
        if ok == 0 {
            tracing::debug!(error = %io::Error::last_os_error(), "instance pipe disconnected");
        }
    }
}

impl Read for Pipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = buffer.len().min(u32::MAX as usize) as u32;
        let mut count = 0;
        // SAFETY: the live synchronous pipe writes at most length bytes into the writable buffer and initializes count.
        let ok = unsafe {
            ReadFile(
                self.0.as_raw_handle(),
                buffer.as_mut_ptr(),
                length,
                &mut count,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(pipe_error());
        }
        Ok(count as usize)
    }
}

impl Write for Pipe {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let length = buffer.len().min(u32::MAX as usize) as u32;
        let mut count = 0;
        // SAFETY: the live synchronous pipe reads at most length bytes from the valid buffer and initializes count.
        let ok = unsafe {
            WriteFile(
                self.0.as_raw_handle(),
                buffer.as_ptr(),
                length,
                &mut count,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(pipe_error());
        }
        if count == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(count as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn pipe_error() -> io::Error {
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(code) if code == ERROR_NO_DATA as i32 || code == ERROR_PIPE_LISTENING as i32 => {
            io::ErrorKind::WouldBlock.into()
        }
        _ => error,
    }
}

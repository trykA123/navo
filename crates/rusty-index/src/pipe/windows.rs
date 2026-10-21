#![allow(unsafe_code, reason = "local named pipes require Win32 FFI")]

mod io;
mod security;

use std::{
    io as std_io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
    sync::{Mutex, PoisonError},
    time::Instant,
};

use serde::{Serialize, de::DeserializeOwned};
use windows_sys::Win32::{
    Foundation::{
        ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED,
        ERROR_SEM_TIMEOUT, HANDLE, INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ,
        FILE_WRITE_DATA, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
        SECURITY_SQOS_PRESENT, WriteFile,
    },
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_REJECT_REMOTE_CLIENTS,
        WaitNamedPipeW,
    },
};

use super::{CONNECT_TIMEOUT, Cancellation, PipeError, frames};
pub(super) use io::CancelEvent;
use io::Operation;
use security::SecurityDescriptor;

pub(super) struct Pipe {
    handle: OwnedHandle,
    incoming: Mutex<Incoming>,
}

struct Incoming {
    bytes: Vec<u8>,
    filled: usize,
}

impl Default for Incoming {
    fn default() -> Self {
        Self {
            bytes: vec![0; 4],
            filled: 0,
        }
    }
}

impl Pipe {
    pub(super) fn connect(name: &str) -> Result<Self, PipeError> {
        let pipe = Self::open(name)?;
        pipe.verify_owner()?;
        Ok(pipe)
    }

    pub(super) fn verify_owner(&self) -> Result<(), PipeError> {
        security::verify_owner(self.raw())
    }

    #[cfg(test)]
    pub(super) fn connect_test(name: &str) -> Result<Self, PipeError> {
        Self::open(name)
    }

    fn open(name: &str) -> Result<Self, PipeError> {
        let name = wide(name);
        let deadline = Instant::now() + CONNECT_TIMEOUT;
        loop {
            // SAFETY: name is NUL-terminated; optional pointers are NULL; the returned handle is uniquely owned.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    FILE_GENERIC_READ | FILE_WRITE_DATA,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self {
                    handle: owned(handle),
                    incoming: Mutex::default(),
                });
            }
            if std_io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_BUSY as i32) {
                return Err(PipeError::NotAvailable);
            }
            let wait = remaining_ms(deadline)?;
            // SAFETY: name stays NUL-terminated and alive for this bounded wait.
            if unsafe { WaitNamedPipeW(name.as_ptr(), wait) } == 0 {
                return Err(connect_error());
            }
            remaining_ms(deadline)?;
        }
    }

    pub(super) fn bind_instances(name: &str, sid: &str) -> Result<Vec<Self>, PipeError> {
        Self::create_instances(name, &SecurityDescriptor::new(sid)?)
    }

    #[cfg(test)]
    pub(super) fn bind_test(name: &str, sid: &str) -> Result<Vec<Self>, PipeError> {
        Self::create_instances(name, &SecurityDescriptor::new_test(sid)?)
    }

    fn create_instances(
        name: &str,
        descriptor: &SecurityDescriptor,
    ) -> Result<Vec<Self>, PipeError> {
        let attributes = descriptor.attributes();
        let name = wide(name);
        (0..4)
            .map(|instance| {
                let first = if instance == 0 {
                    FILE_FLAG_FIRST_PIPE_INSTANCE
                } else {
                    0
                };
                // SAFETY: every instance uses the same live, non-NULL security descriptor and NUL-terminated name.
                let handle = unsafe {
                    CreateNamedPipeW(
                        name.as_ptr(),
                        PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | first,
                        PIPE_REJECT_REMOTE_CLIENTS,
                        4,
                        64 * 1024,
                        64 * 1024,
                        50,
                        &attributes,
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    return Err(io_error());
                }
                Ok(Self {
                    handle: owned(handle),
                    incoming: Mutex::default(),
                })
            })
            .collect()
    }

    pub(super) fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        loop {
            let result = self.accept_once(cancel);
            if matches!(result, Err(PipeError::NotAvailable)) && !cancel.is_cancelled() {
                self.disconnect()?;
                continue;
            }
            return result;
        }
    }

    fn accept_once(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        Operation::new(self)?
            .run(
                |state| {
                    // SAFETY: the operation pins OVERLAPPED through completion or cancellation and draining.
                    unsafe { ConnectNamedPipe(self.raw(), state) }
                },
                None,
                cancel,
            )
            .map(|_| ())
    }

    pub(super) fn disconnect(&self) -> Result<(), PipeError> {
        // SAFETY: all session I/O has completed or been cancelled and drained before disconnecting this owned pipe.
        if unsafe { DisconnectNamedPipe(self.raw()) } == 0
            && std_io::Error::last_os_error().raw_os_error()
                != Some(ERROR_PIPE_NOT_CONNECTED as i32)
        {
            return Err(io_error());
        }
        *self.incoming.lock().unwrap_or_else(PoisonError::into_inner) = Incoming::default();
        Ok(())
    }

    fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle()
    }

    pub(super) fn read<T: DeserializeOwned>(
        &self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<T, PipeError> {
        let mut incoming = self.incoming.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if cancel.is_cancelled() {
                return Err(PipeError::Cancelled);
            }
            if incoming.filled == 4 && incoming.bytes.len() == 4 {
                let length = frames::payload_length(&incoming.bytes)?;
                incoming.bytes.resize(length + 4, 0);
            }
            if incoming.filled == incoming.bytes.len() {
                let result = frames::decode(&incoming.bytes).map_err(PipeError::from);
                *incoming = Incoming::default();
                return result;
            }
            let filled = incoming.filled;
            let count = self.read_some(&mut incoming.bytes[filled..], deadline, cancel);
            match count {
                Ok(0) | Err(PipeError::NotAvailable) if incoming.filled > 0 => {
                    return Err(frames::FrameError::Truncated.into());
                }
                Ok(0) => return Err(PipeError::NotAvailable),
                other => incoming.filled += other?,
            }
        }
    }

    fn read_some(
        &self,
        bytes: &mut [u8],
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<usize, PipeError> {
        let count = Operation::new(self)?.run(
            |state| {
                // SAFETY: bytes remains writable through completion or cancellation and draining.
                unsafe {
                    ReadFile(
                        self.raw(),
                        bytes.as_mut_ptr(),
                        bytes.len() as u32,
                        ptr::null_mut(),
                        state,
                    )
                }
            },
            deadline,
            cancel,
        )? as usize;
        if count > bytes.len() {
            return Err(PipeError::InvalidResponse);
        }
        Ok(count)
    }

    pub(super) fn write(
        &self,
        value: &impl Serialize,
        deadline: Option<Instant>,
    ) -> Result<(), PipeError> {
        let frame = frames::encode(value)?;
        self.write_bytes(&frame, deadline)
    }

    pub(super) fn write_bytes(
        &self,
        mut bytes: &[u8],
        deadline: Option<Instant>,
    ) -> Result<(), PipeError> {
        while !bytes.is_empty() {
            let count = self.write_some(bytes, deadline)?;
            if count == 0 || count > bytes.len() {
                return Err(PipeError::NotAvailable);
            }
            bytes = &bytes[count..];
        }
        Ok(())
    }
    fn write_some(&self, bytes: &[u8], deadline: Option<Instant>) -> Result<usize, PipeError> {
        Operation::new(self)?
            .run(
                |state| {
                    // SAFETY: bytes stays readable through completion or cancellation and draining.
                    unsafe {
                        WriteFile(
                            self.raw(),
                            bytes.as_ptr(),
                            bytes.len() as u32,
                            ptr::null_mut(),
                            state,
                        )
                    }
                },
                deadline,
                &Cancellation::Never,
            )
            .map(|count| count as usize)
    }
}

fn owned(handle: HANDLE) -> OwnedHandle {
    // SAFETY: callers pass a newly created, valid, uniquely owned kernel handle exactly once.
    unsafe { OwnedHandle::from_raw_handle(handle) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn remaining_ms(deadline: Instant) -> Result<u32, PipeError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(PipeError::TimedOut)?;
    Ok(remaining.as_millis().clamp(1, u128::from(u32::MAX - 1)) as u32)
}

fn io_error() -> PipeError {
    let error = std_io::Error::last_os_error();
    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED) => {
            PipeError::NotAvailable
        }
        _ => PipeError::Io(error),
    }
}

fn connect_error() -> PipeError {
    if std_io::Error::last_os_error().raw_os_error() == Some(ERROR_SEM_TIMEOUT as i32) {
        PipeError::TimedOut
    } else {
        PipeError::NotAvailable
    }
}

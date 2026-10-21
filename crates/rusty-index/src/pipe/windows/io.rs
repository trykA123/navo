use std::{
    io,
    os::windows::io::{AsRawHandle, OwnedHandle},
    ptr,
    sync::OnceLock,
    time::Instant,
};

use windows_sys::Win32::{
    Foundation::{ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
    },
};

use super::super::{Cancellation, PipeError};
use super::{Pipe, io_error, owned, remaining_ms};

#[derive(Default)]
pub(in crate::pipe) struct CancelEvent(OnceLock<Result<OwnedHandle, i32>>);

impl CancelEvent {
    pub(in crate::pipe) fn signal(&self) {
        if let Some(Ok(event)) = self.0.get() {
            // SAFETY: the shared guard keeps this event live until every token clone is dropped.
            unsafe { SetEvent(event.as_raw_handle()) };
        }
    }

    fn raw(&self) -> Result<HANDLE, PipeError> {
        self.0
            .get_or_init(create_event)
            .as_ref()
            .map(AsRawHandle::as_raw_handle)
            .map_err(|code| PipeError::Io(io::Error::from_raw_os_error(*code)))
    }
}

fn create_event() -> Result<OwnedHandle, i32> {
    // SAFETY: the unnamed manual-reset event is initially unsignalled and uniquely owned.
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    if event.is_null() {
        return Err(io::Error::last_os_error().raw_os_error().unwrap_or(1));
    }
    Ok(owned(event))
}

pub(super) struct Operation<'a> {
    pipe: &'a Pipe,
    state: Box<OVERLAPPED>,
    _event: OwnedHandle,
    pending: bool,
}

impl<'a> Operation<'a> {
    pub(super) fn new(pipe: &'a Pipe) -> Result<Self, PipeError> {
        let event =
            create_event().map_err(|code| PipeError::Io(io::Error::from_raw_os_error(code)))?;
        Ok(Self {
            pipe,
            state: Box::new(OVERLAPPED {
                hEvent: event.as_raw_handle(),
                ..OVERLAPPED::default()
            }),
            _event: event,
            pending: false,
        })
    }

    pub(super) fn run(
        &mut self,
        start: impl FnOnce(*mut OVERLAPPED) -> i32,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<u32, PipeError> {
        if cancel.is_cancelled() {
            return Err(PipeError::Cancelled);
        }
        if let Some(deadline) = deadline {
            remaining_ms(deadline)?;
        }
        if start(&mut *self.state) == 0 {
            match io::Error::last_os_error()
                .raw_os_error()
                .map(|code| code as u32)
            {
                Some(ERROR_PIPE_CONNECTED) => return Ok(0),
                Some(ERROR_IO_PENDING) => {}
                _ => return Err(io_error()),
            }
        }
        self.pending = true;
        self.wait(deadline, cancel)
    }

    fn wait(
        &mut self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<u32, PipeError> {
        let events = wait_events(self.state.hEvent, cancel)?;
        loop {
            if cancel.is_cancelled() {
                return self.abort();
            }
            let wait = deadline.map(remaining_ms).transpose()?.unwrap_or(INFINITE);
            let wait = if matches!(cancel, Cancellation::Poll(_)) {
                wait.min(10)
            } else {
                wait
            };
            // SAFETY: every event remains live through the wait; the vector contains distinct valid handles.
            let signalled =
                unsafe { WaitForMultipleObjects(events.len() as u32, events.as_ptr(), 0, wait) };
            match signalled {
                WAIT_OBJECT_0 => return self.complete(),
                WAIT_TIMEOUT => continue,
                found if found < events.len() as u32 => {
                    return self.abort();
                }
                _ => return Err(io_error()),
            }
        }
    }

    fn complete(&mut self) -> Result<u32, PipeError> {
        let mut transferred = 0;
        // SAFETY: the event signals completion; the handle and pinned OVERLAPPED remain live.
        let completed =
            unsafe { GetOverlappedResult(self.pipe.raw(), &*self.state, &mut transferred, 0) };
        self.pending = false;
        if completed == 0 {
            return Err(io_error());
        }
        Ok(transferred)
    }

    fn abort(&mut self) -> Result<u32, PipeError> {
        let mut transferred = 0;
        // SAFETY: draining this operation keeps its buffer, OVERLAPPED and event alive until the kernel releases them.
        let completed = unsafe {
            CancelIoEx(self.pipe.raw(), &*self.state);
            GetOverlappedResult(self.pipe.raw(), &*self.state, &mut transferred, 1)
        };
        self.pending = false;
        if completed != 0 {
            return Ok(transferred);
        }
        Err(PipeError::Cancelled)
    }
}

fn wait_events(event: HANDLE, cancel: &Cancellation<'_>) -> Result<Vec<HANDLE>, PipeError> {
    let mut events = vec![event];
    if let Cancellation::Tokens(tokens) = cancel {
        for token in *tokens {
            let event = token.event.raw()?;
            if token.is_cancelled() {
                token.event.signal();
            }
            if !events.contains(&event) {
                events.push(event);
            }
        }
    }
    Ok(events)
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        if self.pending {
            let _ = self.abort();
        }
    }
}

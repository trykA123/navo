#![allow(unsafe_code, reason = "named change mutexes require Win32 FFI")]

use std::io;
use std::marker::PhantomData;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::rc::Rc;
use std::time::Duration;

use windows_sys::Win32::Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use super::{Held, Scope, WaitResult};

/// Serialises app service changes in one session. Drop on the acquiring thread.
pub struct ChangeLock {
    _held: Held<Mutex>,
}

impl ChangeLock {
    /// Waits at most `wait`; zero tries once. `None` means the mutex is still held.
    pub fn acquire(scope: Scope, wait: Duration) -> io::Result<Option<Self>> {
        let name: Vec<u16> = scope.name().encode_utf16().chain(Some(0)).collect();
        // SAFETY: name is terminated and live; default security and no initial ownership are requested.
        let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateMutexW returned a new handle that only this owner closes.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mutex = Mutex {
            handle,
            _thread: PhantomData,
        };
        Held::acquire(mutex, wait).map(|held| held.map(|held| Self { _held: held }))
    }
}

struct Mutex {
    handle: OwnedHandle,
    _thread: PhantomData<Rc<()>>,
}

impl super::Mutex for Mutex {
    fn wait(&self, wait: Duration) -> io::Result<WaitResult> {
        let millis = u32::try_from(wait.as_millis())
            .ok()
            .filter(|&millis| millis != u32::MAX)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "mutex wait too long"))?;
        // SAFETY: handle is live and has wait access; millis is a finite timeout.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), millis) } {
            WAIT_OBJECT_0 => Ok(WaitResult::Acquired),
            WAIT_ABANDONED => Ok(WaitResult::Abandoned),
            WAIT_TIMEOUT => Ok(WaitResult::TimedOut),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn release(&self) -> io::Result<()> {
        // SAFETY: a Held guard releases exactly once on the thread that acquired this mutex.
        if unsafe { ReleaseMutex(self.handle.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

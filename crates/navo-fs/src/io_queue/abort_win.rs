//! Abort handles for network jobs: a worker on a network volume stores a handle to its own
//! thread, so `cancel_owner` can interrupt a call blocked on a dead share (NV5b).
#![cfg_attr(
    windows,
    allow(
        unsafe_code,
        reason = "std has no safe wrapper for OpenThread and CancelSynchronousIo"
    )
)]

use std::collections::HashMap;

use super::JobId;

pub(super) use platform::ThreadAbort;

/// Interrupts the synchronous I/O of one thread.
pub(super) trait Abort {
    /// Asks the thread's blocked call to fail now; a thread with no pending call is unaffected.
    fn abort(&self);
}

/// Abort handles of running network jobs, keyed by job. Every call happens under the queue's
/// state lock, so a handle is removed before its thread can move on to another job.
pub(super) struct AbortStore<H: Abort = ThreadAbort> {
    handles: HashMap<JobId, H>,
}

impl<H: Abort> AbortStore<H> {
    /// Stores the handle of the thread now running `job`.
    pub(super) fn insert(&mut self, job: JobId, handle: H) {
        self.handles.insert(job, handle);
    }

    /// Forgets `job`'s handle when the job ends; dropping it closes the thread handle.
    pub(super) fn remove(&mut self, job: JobId) {
        self.handles.remove(&job);
    }

    /// Aborts the stored handles of `jobs`; jobs with no handle (not on a network) are skipped.
    pub(super) fn abort(&self, jobs: &[JobId]) {
        jobs.iter()
            .filter_map(|job| self.handles.get(job))
            .for_each(Abort::abort);
    }

    /// Aborts every stored handle, for shutdown.
    pub(super) fn abort_all(&self) {
        self.handles.values().for_each(Abort::abort);
    }

    /// Number of stored handles.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.handles.len()
    }
}

impl<H: Abort> Default for AbortStore<H> {
    fn default() -> Self {
        Self {
            handles: HashMap::new(),
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::IO::CancelSynchronousIo;
    use windows_sys::Win32::System::Threading::{GetCurrentThreadId, OpenThread, THREAD_TERMINATE};

    /// An owned handle to one worker thread, opened with the right `CancelSynchronousIo` needs.
    pub(in crate::io_queue) struct ThreadAbort(HANDLE);

    // SAFETY: a thread handle is a kernel object reference that any thread may use or close.
    unsafe impl Send for ThreadAbort {}

    impl ThreadAbort {
        /// Opens a handle to the calling thread; `None` when the OS refuses it.
        pub(in crate::io_queue) fn current() -> Option<Self> {
            // SAFETY: plain Win32 calls with no pointers; a null result is handled below.
            let handle = unsafe { OpenThread(THREAD_TERMINATE, 0, GetCurrentThreadId()) };
            (!handle.is_null()).then(|| Self(handle))
        }
    }

    impl super::Abort for ThreadAbort {
        fn abort(&self) {
            // SAFETY: the handle is open until Drop; with no pending call this fails harmlessly.
            unsafe { CancelSynchronousIo(self.0) };
        }
    }

    impl Drop for ThreadAbort {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenThread and is closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    /// No-op on platforms without `CancelSynchronousIo`: nothing is ever stored.
    pub(in crate::io_queue) struct ThreadAbort;

    impl ThreadAbort {
        /// Always `None`: there is no thread I/O to cancel here.
        pub(in crate::io_queue) fn current() -> Option<Self> {
            None
        }
    }

    impl super::Abort for ThreadAbort {
        fn abort(&self) {}
    }
}

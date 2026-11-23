//! The PDF helper's Job Object: a memory cap, kill-on-close and one process, so a helper that
//! misbehaves dies with Navo and cannot spawn children (S2; `spawn_win.rs` starts it inside).

use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Child;
use std::ptr;

use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};

use super::HelperLimits;

const INFO_BYTES: u32 = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;

/// An owned Job handle; closing it kills every process in the Job.
pub struct HelperJob(OwnedHandle);

/// Creates a memory-capped Job and assigns the running `child` to it. Fails on any Job error.
pub fn confine_helper(child: &Child, limits: HelperLimits) -> io::Result<HelperJob> {
    let job = HelperJob::create(limits)?;
    job.assign(child)?;
    Ok(job)
}

impl HelperJob {
    /// Creates an unnamed Job with the memory cap in `limits` (bytes), kill-on-close, death on
    /// an unhandled exception and one active process. Fails when Windows refuses either call.
    pub(super) fn create(limits: HelperLimits) -> io::Result<Self> {
        // SAFETY: null attributes and a null name create an unnamed job with default security; nothing is read from our memory.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `raw` is a job handle this call just created and nothing else owns.
        let job = HelperJob(unsafe { OwnedHandle::from_raw_handle(raw) });
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = 1;
        info.ProcessMemoryLimit = limits.memory_bytes;
        // SAFETY: the job handle is live and `info` is a live JOBOBJECT_EXTENDED_LIMIT_INFORMATION of INFO_BYTES bytes.
        let set = unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                INFO_BYTES,
            )
        };
        check(set)?;
        Ok(job)
    }

    /// Puts the process behind `process` into this Job. Fails when Windows refuses it.
    pub(super) fn assign(&self, process: &impl AsHandle) -> io::Result<()> {
        let process = process.as_handle().as_raw_handle();
        // SAFETY: both handles are live for the call; the caller keeps owning its process handle.
        check(unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), process) })
    }

    /// The Job's memory cap per process in bytes. Fails when the query fails.
    pub fn memory_limit(&self) -> io::Result<usize> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: the job handle is live and `info` is a live, writable buffer of INFO_BYTES bytes; the length out-pointer may be null.
        let queried = unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                INFO_BYTES,
                ptr::null_mut(),
            )
        };
        check(queried)?;
        Ok(info.ProcessMemoryLimit)
    }

    /// Whether the process behind `process` belongs to this Job. Fails when the query fails.
    pub fn contains(&self, process: &impl AsHandle) -> io::Result<bool> {
        let process = process.as_handle().as_raw_handle();
        let mut result = 0;
        // SAFETY: both handles are live for the call and `result` is a live, writable BOOL.
        let ok = unsafe { IsProcessInJob(process, self.0.as_raw_handle(), &raw mut result) };
        check(ok)?;
        Ok(result != 0)
    }
}

/// Turns a Win32 `BOOL` result into `Ok`, or the last OS error when it is zero.
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

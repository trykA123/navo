use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
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

pub struct HelperJob(OwnedHandle);

pub fn confine_helper(child: &Child, limits: HelperLimits) -> io::Result<HelperJob> {
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
    // SAFETY: both handles are live for the call; `child` keeps owning its process handle.
    let assigned =
        unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), child.as_raw_handle()) };
    check(assigned)?;
    Ok(job)
}

impl HelperJob {
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

    pub fn contains(&self, child: &Child) -> io::Result<bool> {
        let mut result = 0;
        // SAFETY: both handles are live for the call and `result` is a live, writable BOOL.
        let ok = unsafe {
            IsProcessInJob(
                child.as_raw_handle(),
                self.0.as_raw_handle(),
                &raw mut result,
            )
        };
        check(ok)?;
        Ok(result != 0)
    }
}

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

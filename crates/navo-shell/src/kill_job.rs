use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::ptr;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

/// A job that holds a process and every process it starts; closing it kills whatever is left.
pub struct KillJob(OwnedHandle);

/// Spawns `command` without a window, suspended, puts it in a new kill-on-close job, then lets
/// it run, so none of its descendants can start outside the job. The job is `None` when Windows
/// refuses one; the process then runs unconfined. A process that cannot be resumed is killed.
pub fn spawn_in_kill_job(command: &mut Command) -> io::Result<(Child, Option<KillJob>)> {
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let mut child = command.spawn()?;
    let job = confine(&child).ok();
    if let Err(error) = resume(&child) {
        if let Some(job) = &job {
            job.terminate().unwrap_or_default();
        }
        child.kill().unwrap_or_default();
        child.wait().map(drop).unwrap_or_default();
        return Err(error);
    }
    Ok((child, job))
}

fn confine(child: &Child) -> io::Result<KillJob> {
    // SAFETY: null attributes and a null name create an unnamed job with default security.
    let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a job handle this call just created and nothing else owns.
    let job = KillJob(unsafe { OwnedHandle::from_raw_handle(raw) });
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let bytes = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: the job handle is live and `info` is a live structure of `bytes` bytes.
    let set = unsafe {
        SetInformationJobObject(
            job.0.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            bytes,
        )
    };
    check(set)?;
    // SAFETY: both handles are live for the call; `child` keeps owning its process handle.
    let assigned =
        unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), child.as_raw_handle()) };
    check(assigned)?;
    Ok(job)
}

impl KillJob {
    /// Kills every process in the job.
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: the job handle is live for the call.
        check(unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) })
    }

    /// How many processes the job still holds.
    pub fn active_processes(&self) -> io::Result<u32> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let bytes = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut returned = 0;
        // SAFETY: the job handle is live and `info` is a writable structure of `bytes` bytes.
        check(unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast(),
                bytes,
                &mut returned,
            )
        })?;
        Ok(info.ActiveProcesses)
    }
}

fn resume(child: &Child) -> io::Result<()> {
    // SAFETY: a thread snapshot takes no pointers; the process id is ignored for threads.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a snapshot handle this call just created and nothing else owns.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        ..THREADENTRY32::default()
    };
    let mut resumed = false;
    // SAFETY: the snapshot handle is live and `entry` is a writable THREADENTRY32 with dwSize set.
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) };
    while found != 0 {
        if entry.th32OwnerProcessID == child.id() {
            resume_thread(entry.th32ThreadID)?;
            resumed = true;
        }
        // SAFETY: as for Thread32First.
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) };
    }
    if resumed {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the suspended git has no thread",
        ))
    }
}

fn resume_thread(id: u32) -> io::Result<()> {
    // SAFETY: OpenThread takes no pointers and returns null on failure.
    let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, id) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a thread handle this call just opened and nothing else owns.
    let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: the thread handle is live and has THREAD_SUSPEND_RESUME access.
    if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

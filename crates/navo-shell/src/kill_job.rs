use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};

/// How often a stopping job's process count is polled.
const POLL: Duration = Duration::from_millis(5);
/// Process ids read per list query before the buffer grows to the job's size.
const FIRST_LIST: usize = 16;

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

    /// Kills every process in the job and waits, at most `timeout`, until each one has fully
    /// exited: its process object is signalled, so it no longer holds files open. A zero active
    /// count alone is not enough, since Windows drops a process from it before closing its
    /// handles. Returns whether all exited; `Err` only when the job could not be terminated.
    ///
    /// A process that starts and dies between the first list and the kill is seen only through
    /// the active count.
    pub fn terminate_and_wait(&self, timeout: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + timeout;
        let mut held = Vec::new();
        let listed = self.hold_members(&mut held);
        self.terminate()?;
        match listed.and_then(|()| self.drain(&mut held, deadline)) {
            Ok(exited) => Ok(exited),
            Err(error) => {
                tracing::warn!(%error, "waiting for the kill job's processes failed");
                Ok(false)
            }
        }
    }

    /// Holds every listed process until the active count is zero, then waits on each handle.
    fn drain(&self, held: &mut Vec<Member>, deadline: Instant) -> io::Result<bool> {
        loop {
            self.hold_members(held)?;
            if self.active_processes()? == 0 {
                break;
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(POLL);
        }
        Ok(held.iter().all(|member| member.exited_by(deadline)))
    }

    /// Opens a wait handle to every listed process not yet held.
    fn hold_members(&self, held: &mut Vec<Member>) -> io::Result<()> {
        for id in self.member_ids()? {
            if held.iter().any(|member| member.id == id) {
                continue;
            }
            if let Some(member) = Member::open(id, &self.0) {
                held.push(member);
            }
        }
        Ok(())
    }

    /// The ids of the processes the job holds now, growing the buffer when Windows asks.
    fn member_ids(&self) -> io::Result<Vec<u32>> {
        let header =
            offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList) / size_of::<usize>();
        let mut capacity = FIRST_LIST;
        for _ in 0..4 {
            let mut buffer = vec![0usize; header + capacity];
            let bytes = u32::try_from(buffer.len() * size_of::<usize>())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            // SAFETY: the job handle is live and `buffer` is a writable, usize-aligned block of
            // `bytes` bytes, large enough for the list header and one id.
            let listed = unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    bytes,
                    ptr::null_mut(),
                )
            };
            let error = check(listed).err();
            // SAFETY: `buffer` starts with a usize-aligned list header that holds at least one id.
            let list = unsafe {
                buffer
                    .as_ptr()
                    .cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
                    .read()
            };
            match error {
                None => {
                    let count = usize::try_from(list.NumberOfProcessIdsInList)
                        .unwrap_or(capacity)
                        .min(capacity);
                    return Ok(buffer[header..header + count]
                        .iter()
                        .filter_map(|id| u32::try_from(*id).ok())
                        .collect());
                }
                Some(error) if is_more_data(&error) => {
                    let needed = usize::try_from(list.NumberOfAssignedProcesses).unwrap_or(0);
                    capacity = needed.max(capacity * 2);
                }
                Some(error) => return Err(error),
            }
        }
        Err(io::Error::other("the job's process list kept growing"))
    }
}

/// A process of a job, held open so its exit can be awaited.
struct Member {
    id: u32,
    process: OwnedHandle,
}

impl Member {
    /// Opens `id` for waiting; `None` when it is gone or the id now names a process outside `job`.
    fn open(id: u32, job: &OwnedHandle) -> Option<Self> {
        // SAFETY: OpenProcess takes no pointers and returns null on failure.
        let raw = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                id,
            )
        };
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is a process handle this call just opened and nothing else owns.
        let process = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut inside = 0;
        // SAFETY: both handles are live for the call and `inside` is a writable BOOL.
        let asked =
            unsafe { IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &mut inside) };
        (asked != 0 && inside != 0).then_some(Self { id, process })
    }

    /// Waits until `deadline` for the process to exit; true once it has.
    fn exited_by(&self, deadline: Instant) -> bool {
        let left = deadline.saturating_duration_since(Instant::now());
        let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the process handle is live and has SYNCHRONIZE access.
        unsafe { WaitForSingleObject(self.process.as_raw_handle(), millis) == WAIT_OBJECT_0 }
    }
}

fn is_more_data(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .and_then(|code| u32::try_from(code).ok())
        == Some(ERROR_MORE_DATA)
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

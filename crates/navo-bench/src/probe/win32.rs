//! Raw Win32 calls for the probes. Every handle is owned by an `OwnedHandle` or by `Child`.

use std::mem::offset_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::null;

use windows_sys::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessInformation,
};
use windows_sys::Win32::Foundation::{
    FILETIME, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM, STATUS_INFO_LENGTH_MISMATCH,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};
use windows_sys::Win32::System::WindowsProgramming::{
    CLIENT_ID, SYSTEM_PROCESS_INFORMATION, SYSTEM_THREAD_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    PostMessageW,
};
use windows_sys::core::BOOL;

use super::keys::{Message, WindowId, WindowInfo, WindowOs};
use super::math::{CpuTime, ThreadSwitches};
use super::sysinfo::{self, Layout};
use super::{Memory, ProbeError, Step};

const SYSINFO_START: usize = 1 << 20;
const SYSINFO_TRIES: usize = 8;

fn last(step: Step) -> ProbeError {
    // SAFETY: reads this thread's last-error value; no arguments.
    let code = unsafe { GetLastError() };
    ProbeError::Os { step, code }
}

/// An unnamed job whose last handle closing kills every process in it.
pub(super) fn kill_on_close_job() -> Result<OwnedHandle, ProbeError> {
    // SAFETY: null attributes and name create an unnamed, non-inheritable job.
    let raw = unsafe { CreateJobObjectW(null(), null()) };
    if raw.is_null() {
        return Err(last(Step::CreateJob));
    }
    // SAFETY: `raw` is a fresh valid handle that nothing else owns.
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let size = u32::try_from(size_of_val(&limits)).unwrap_or(u32::MAX);
    // SAFETY: `limits` is live and `size` is its exact size for this information class.
    let ok = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(last(Step::LimitJob));
    }
    Ok(job)
}

/// Whether `process` runs inside `job`.
#[cfg(test)]
pub(super) fn is_in_job(process: RawHandle, job: &OwnedHandle) -> Result<bool, ProbeError> {
    let mut inside: BOOL = 0;
    // SAFETY: both handles are open; `inside` is a live output.
    let ok = unsafe {
        windows_sys::Win32::System::JobObjects::IsProcessInJob(
            process,
            job.as_raw_handle(),
            &raw mut inside,
        )
    };
    if ok == 0 {
        return Err(last(Step::AssignJob));
    }
    Ok(inside != 0)
}

pub(super) fn assign(job: &OwnedHandle, process: RawHandle) -> Result<(), ProbeError> {
    // SAFETY: both handles are open for the whole call.
    let ok = unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) };
    if ok == 0 {
        return Err(last(Step::AssignJob));
    }
    Ok(())
}

/// Resumes the single thread of a process started suspended.
pub(super) fn resume_only_thread(pid: u32) -> Result<(), ProbeError> {
    let threads = thread_ids(pid)?;
    let [thread_id] = threads[..] else {
        return Err(ProbeError::Os {
            step: Step::Resume,
            code: 0,
        });
    };
    // SAFETY: plain open by ID; a null result is checked below.
    let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
    if raw.is_null() {
        return Err(last(Step::Resume));
    }
    // SAFETY: `raw` is a fresh valid handle that nothing else owns.
    let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
    // SAFETY: `thread` is open with THREAD_SUSPEND_RESUME.
    if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
        return Err(last(Step::Resume));
    }
    Ok(())
}

fn thread_ids(pid: u32) -> Result<Vec<u32>, ProbeError> {
    // SAFETY: a system-wide thread snapshot; the result is checked below.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE || raw.is_null() {
        return Err(last(Step::Resume));
    }
    // SAFETY: `raw` is a fresh valid handle that nothing else owns.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(0),
        ..THREADENTRY32::default()
    };
    let mut ids = Vec::new();
    // SAFETY: `entry` is live with its size field set; the snapshot is open.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            ids.push(entry.th32ThreadID);
        }
        // SAFETY: as for Thread32First.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) } != 0;
    }
    Ok(ids)
}

pub(super) fn memory(process: RawHandle) -> Result<Memory, ProbeError> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(0),
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    // SAFETY: the EX struct is live and `cb` tells the call its size.
    let ok = unsafe { GetProcessMemoryInfo(process, (&raw mut counters).cast(), counters.cb) };
    if ok == 0 {
        return Err(last(Step::Memory));
    }
    Ok(Memory {
        private_bytes: counters.PrivateUsage as u64,
        working_set_bytes: counters.WorkingSetSize as u64,
    })
}

pub(super) fn cpu_time(process: RawHandle) -> Result<CpuTime, ProbeError> {
    let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
    // SAFETY: four live FILETIME outputs; the handle is open.
    let ok = unsafe {
        GetProcessTimes(
            process,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    };
    if ok == 0 {
        return Err(last(Step::Times));
    }
    Ok(CpuTime::from_filetimes(
        (kernel.dwLowDateTime, kernel.dwHighDateTime),
        (user.dwLowDateTime, user.dwHighDateTime),
    ))
}

/// The struct layout of this build, from the windows-sys definitions.
pub(super) fn layout() -> Layout {
    Layout {
        pointer: size_of::<usize>(),
        process_size: size_of::<SYSTEM_PROCESS_INFORMATION>(),
        process_id: offset_of!(SYSTEM_PROCESS_INFORMATION, UniqueProcessId),
        thread_size: size_of::<SYSTEM_THREAD_INFORMATION>(),
        thread_id: offset_of!(SYSTEM_THREAD_INFORMATION, ClientId)
            + offset_of!(CLIENT_ID, UniqueThread),
        switches: offset_of!(SYSTEM_THREAD_INFORMATION, Reserved3),
    }
}

pub(super) fn thread_switches(pid: u32) -> Result<Vec<ThreadSwitches>, ProbeError> {
    let buffer = process_snapshot()?;
    sysinfo::threads_of(&buffer, &layout(), pid)
}

fn process_snapshot() -> Result<Vec<u8>, ProbeError> {
    let mut words = SYSINFO_START / 8;
    for _ in 0..SYSINFO_TRIES {
        // u64 words keep the buffer 8-byte aligned as the call requires.
        let mut buffer = vec![0_u64; words];
        let bytes = u32::try_from(words * 8).map_err(|_| ProbeError::Malformed)?;
        let mut needed = 0_u32;
        // SAFETY: `buffer` is live, writable and `bytes` long; `needed` is a live output.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buffer.as_mut_ptr().cast(),
                bytes,
                &raw mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            words = (needed as usize).max(words * 8) / 8 * 2;
            continue;
        }
        if status < 0 {
            return Err(ProbeError::Os {
                step: Step::Switches,
                code: u32::from_ne_bytes(status.to_ne_bytes()),
            });
        }
        let used = (needed as usize).min(words * 8);
        let mut flat: Vec<u8> = buffer.iter().flat_map(|word| word.to_ne_bytes()).collect();
        flat.truncate(used);
        return Ok(flat);
    }
    Err(ProbeError::Os {
        step: Step::Switches,
        code: u32::from_ne_bytes(STATUS_INFO_LENGTH_MISMATCH.to_ne_bytes()),
    })
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec` that `top_windows` lends for this EnumWindows call.
    let list = unsafe { &mut *(lparam as *mut Vec<WindowInfo>) };
    let mut pid = 0_u32;
    // SAFETY: `hwnd` comes from EnumWindows; `pid` is a live output.
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
    // SAFETY: `hwnd` comes from EnumWindows.
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    // SAFETY: `hwnd` comes from EnumWindows.
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    list.push(WindowInfo {
        id: WindowId(hwnd as isize),
        pid,
        visible,
        owned,
    });
    1
}

/// Every top-level window in z-order, top first.
pub(super) fn top_windows() -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = Vec::new();
    // SAFETY: `collect` only touches `list`, which outlives this synchronous call.
    unsafe { EnumWindows(Some(collect), (&raw mut list) as LPARAM) };
    list
}

/// Win32 window queries and posting.
pub(super) struct Win32Windows;

impl WindowOs for Win32Windows {
    fn window_pid(&self, window: WindowId) -> Option<u32> {
        let hwnd = window.0 as HWND;
        // SAFETY: IsWindow accepts any value and reports whether it names a window.
        if unsafe { IsWindow(hwnd) } == 0 {
            return None;
        }
        let mut pid = 0_u32;
        // SAFETY: `pid` is a live output; a stale handle yields thread 0, checked below.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
        (thread != 0).then_some(pid)
    }

    fn post(&self, window: WindowId, message: Message) -> Result<(), ProbeError> {
        // SAFETY: posting copies plain integers; the caller verified the window's process.
        let ok =
            unsafe { PostMessageW(window.0 as HWND, message.id, message.wparam, message.lparam) };
        if ok == 0 {
            return Err(last(Step::Post));
        }
        Ok(())
    }

    fn scan_code(&self, virtual_key: u16) -> u8 {
        // SAFETY: a pure table lookup in the current keyboard layout.
        let scan = unsafe { MapVirtualKeyW(u32::from(virtual_key), MAPVK_VK_TO_VSC) };
        u8::try_from(scan & 0xFF).unwrap_or(0)
    }
}

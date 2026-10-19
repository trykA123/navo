#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for Restart Manager or process image queries"
)]

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::ptr;

use rusty_core::LockHolder;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_MORE_DATA, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
    RmStartSession,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};

const LIST_ATTEMPTS: usize = 4;
const IMAGE_PATH_UNITS: u32 = 32_768;

pub(super) fn holders(path: &Path) -> Vec<LockHolder> {
    lookup(path).unwrap_or_default()
}

fn lookup(path: &Path) -> Option<Vec<LockHolder>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return None;
    }
    wide.push(0);
    let session = Session::start()?;
    session.register(&wide)?;
    let processes = session.processes()?;
    // SAFETY: GetCurrentProcessId has no preconditions.
    let own_pid = unsafe { GetCurrentProcessId() };
    Some(
        processes
            .iter()
            .filter(|process| process.Process.dwProcessId != own_pid)
            .filter_map(lock_holder)
            .collect(),
    )
}

struct Session(u32);

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: this guard owns a session successfully created by RmStartSession.
        unsafe { RmEndSession(self.0) };
    }
}

impl Session {
    fn start() -> Option<Self> {
        let mut handle = 0;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: handle is writable and key has CCH_RM_SESSION_KEY + 1 UTF-16 units.
        let status = unsafe { RmStartSession(&raw mut handle, 0, key.as_mut_ptr()) };
        if status != ERROR_SUCCESS {
            return None;
        }
        Some(Self(handle))
    }

    fn register(&self, path: &[u16]) -> Option<()> {
        let files = [path.as_ptr()];
        // SAFETY: the live session registers one NUL-terminated path; other arrays have zero length.
        let status = unsafe {
            RmRegisterResources(self.0, 1, files.as_ptr(), 0, ptr::null(), 0, ptr::null())
        };
        (status == ERROR_SUCCESS).then_some(())
    }

    fn processes(&self) -> Option<Vec<RM_PROCESS_INFO>> {
        let mut processes = Vec::new();
        for _ in 0..LIST_ATTEMPTS {
            let mut needed = 0;
            let mut count = u32::try_from(processes.len()).ok()?;
            let mut reasons = 0;
            let buffer = if processes.is_empty() {
                ptr::null_mut()
            } else {
                processes.as_mut_ptr()
            };
            // SAFETY: all outputs are writable; buffer is null or holds count initialized records.
            let status = unsafe {
                RmGetList(
                    self.0,
                    &raw mut needed,
                    &raw mut count,
                    buffer,
                    &raw mut reasons,
                )
            };
            if status == ERROR_SUCCESS {
                processes.truncate(count as usize);
                return Some(processes);
            }
            if status != ERROR_MORE_DATA {
                return None;
            }
            processes.resize(needed as usize, RM_PROCESS_INFO::default());
        }
        None
    }
}

fn lock_holder(process: &RM_PROCESS_INFO) -> Option<LockHolder> {
    let pid = process.Process.dwProcessId;
    let end = process
        .strAppName
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(process.strAppName.len());
    let name = if end == 0 {
        executable_name(pid)?
    } else {
        String::from_utf16_lossy(&process.strAppName[..end])
    };
    Some(LockHolder { name, pid })
}

struct Process(HANDLE);

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: this guard owns a non-null process handle returned by OpenProcess.
        unsafe { CloseHandle(self.0) };
    }
}

fn executable_name(pid: u32) -> Option<String> {
    // SAFETY: requests only query access for a process ID, with no inherited handle.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let process = Process(handle);
    let mut path = vec![0u16; IMAGE_PATH_UNITS as usize];
    let mut length = IMAGE_PATH_UNITS;
    // SAFETY: process owns a live query handle and path has length writable UTF-16 units.
    let success = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &raw mut length,
        )
    };
    if success == 0 {
        return None;
    }
    let path = OsString::from_wide(path.get(..length as usize)?);
    let name = Path::new(&path).file_name()?.to_string_lossy().into_owned();
    (!name.is_empty()).then_some(name)
}

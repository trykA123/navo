#[cfg(not(windows))]
pub(super) fn sample() -> Vec<rusty_diag::perf::ThreadTime> {
    Vec::new()
}

#[cfg(windows)]
pub(super) use win::sample;

#[cfg(windows)]
mod win {
    use std::time::Duration;

    use rusty_diag::perf::ThreadTime;
    use windows_sys::Win32::Foundation::{
        CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetThreadDescription, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::core::PWSTR;

    const NAME_LIMIT: usize = 256;

    pub(in super::super) fn sample() -> Vec<ThreadTime> {
        let own = std::process::id();
        let Ok(size) = u32::try_from(size_of::<THREADENTRY32>()) else {
            return Vec::new();
        };
        // SAFETY: no pointers are passed; the result is checked before use and closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Vec::new();
        }
        let mut entry = THREADENTRY32 {
            dwSize: size,
            ..THREADENTRY32::default()
        };
        let mut threads = Vec::new();
        // SAFETY: `snapshot` is a live toolhelp handle and `entry` is a writable THREADENTRY32 with dwSize set.
        let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while more {
            if entry.th32OwnerProcessID == own
                && let Some(thread) = thread_time(entry.th32ThreadID)
            {
                threads.push(thread);
            }
            entry.dwSize = size;
            // SAFETY: as above; `snapshot` stays open until the loop ends.
            more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }
        // SAFETY: `snapshot` came from CreateToolhelp32Snapshot above and is closed exactly once.
        unsafe { CloseHandle(snapshot) };
        threads
    }

    fn thread_time(id: u32) -> Option<ThreadTime> {
        // SAFETY: no pointers are passed; a null result is checked and a live handle is closed below.
        let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, id) };
        if handle.is_null() {
            return None;
        }
        let cpu = cpu_time(handle);
        let name = name(handle);
        // SAFETY: `handle` came from OpenThread above and is closed exactly once.
        unsafe { CloseHandle(handle) };
        Some(ThreadTime {
            id,
            name,
            cpu: cpu?,
        })
    }

    fn cpu_time(handle: HANDLE) -> Option<Duration> {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: `handle` is a live thread handle with query access, and all four pointers are live, writable FILETIMEs.
        let ok = unsafe {
            GetThreadTimes(
                handle,
                &raw mut created,
                &raw mut exited,
                &raw mut kernel,
                &raw mut user,
            )
        };
        if ok == 0 {
            return None;
        }
        let steps =
            |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        let total = steps(kernel).checked_add(steps(user))?;
        Some(Duration::from_nanos(total.saturating_mul(100)))
    }

    fn name(handle: HANDLE) -> Option<String> {
        let mut text: PWSTR = std::ptr::null_mut();
        // SAFETY: `handle` is a live thread handle with query access and `text` is a writable PWSTR slot.
        let result = unsafe { GetThreadDescription(handle, &raw mut text) };
        if result < 0 || text.is_null() {
            return None;
        }
        let mut len = 0;
        // SAFETY: on success `text` is a NUL-terminated wide string; reading stops at the NUL or the limit.
        while len < NAME_LIMIT && unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the first `len` u16 values before the NUL were read above and stay allocated until LocalFree.
        let name = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY: `text` was allocated by GetThreadDescription and is freed exactly once.
        unsafe { LocalFree(text.cast()) };
        let name = name.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

use std::time::Duration;

pub(super) fn age_from_file_times(created: u64, now: u64) -> Option<Duration> {
    let steps = now.checked_sub(created)?;
    Some(Duration::from_nanos(steps.saturating_mul(100)))
}

#[cfg(windows)]
pub(crate) fn process_age() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the pseudo-handle of the current process needs no closing, and all four pointers are live, writable FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    };
    if ok == 0 {
        return None;
    }
    let mut now = FILETIME::default();
    // SAFETY: `now` is a live, writable FILETIME.
    unsafe { GetSystemTimePreciseAsFileTime(&raw mut now) };
    let steps =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    age_from_file_times(steps(created), steps(now))
}

#[cfg(windows)]
pub(super) fn peak_working_set() -> Option<u64> {
    crate::windows_memory::peak_working_set()
}

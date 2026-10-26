use std::time::Duration;

#[cfg(any(unix, test))]
const TICKS_PER_SECOND: u64 = 100;

#[cfg(any(unix, test))]
fn stat_fields(stat: &str) -> Option<Vec<&str>> {
    Some(stat[stat.rfind(')')? + 1..].split_whitespace().collect())
}

#[cfg(any(unix, test))]
pub(super) fn cpu_from_stat(stat: &str) -> Option<Duration> {
    let fields = stat_fields(stat)?;
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    let ticks = user.checked_add(system)?;
    Some(Duration::from_millis(
        ticks.checked_mul(1000)? / TICKS_PER_SECOND,
    ))
}

#[cfg(any(unix, test))]
pub(super) fn threads_from_stat(stat: &str) -> Option<u32> {
    stat_fields(stat)?.get(17)?.parse().ok()
}

#[cfg(unix)]
pub(super) fn cpu_time() -> Option<Duration> {
    cpu_from_stat(&std::fs::read_to_string("/proc/self/stat").ok()?)
}

#[cfg(unix)]
pub(super) fn threads() -> Option<u32> {
    threads_from_stat(&std::fs::read_to_string("/proc/self/stat").ok()?)
}

#[cfg(windows)]
pub(super) fn cpu_time() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
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
    let steps =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    let total = steps(kernel).checked_add(steps(user))?;
    Some(Duration::from_nanos(total.saturating_mul(100)))
}

#[cfg(windows)]
pub(super) fn threads() -> Option<u32> {
    None
}

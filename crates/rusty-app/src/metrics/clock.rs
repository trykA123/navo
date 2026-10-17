use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", test))]
const TICKS_PER_SECOND: f64 = 100.0;

#[derive(Debug, Clone, Copy)]
pub struct ProcessClock {
    start: Instant,
}

impl ProcessClock {
    pub fn starting_at(start: Instant) -> Self {
        Self { start }
    }

    pub fn detect(main_started: Instant) -> Self {
        let now = Instant::now();
        let start = process_age()
            .and_then(|age| now.checked_sub(age))
            .map_or(main_started, |start| start.min(main_started));
        Self::starting_at(start)
    }

    pub fn start(&self) -> Instant {
        self.start
    }

    pub fn ms(&self, at: Instant) -> f64 {
        let ms = at.saturating_duration_since(self.start).as_secs_f64() * 1000.0;
        (ms * 10.0).round() / 10.0
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn age_from_proc(stat: &str, uptime: &str) -> Option<Duration> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    let started_ticks: f64 = after_name.split_whitespace().nth(19)?.parse().ok()?;
    let up_seconds: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    let age = up_seconds - started_ticks / TICKS_PER_SECOND;
    age.is_finite()
        .then(|| Duration::from_secs_f64(age.max(0.0)))
}

#[cfg(any(windows, test))]
pub fn age_from_filetimes(created: u64, now: u64) -> Option<Duration> {
    let steps = now.checked_sub(created)?;
    Some(Duration::from_nanos(steps.saturating_mul(100)))
}

#[cfg(target_os = "linux")]
pub fn process_age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    age_from_proc(&stat, &uptime)
}

#[cfg(windows)]
pub fn process_age() -> Option<Duration> {
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
    age_from_filetimes(steps(created), steps(now))
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn process_age() -> Option<Duration> {
    None
}

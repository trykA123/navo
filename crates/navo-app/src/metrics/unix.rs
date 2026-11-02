use std::time::Duration;

const TICKS_PER_SECOND: f64 = 100.0;

pub(super) fn age_from_proc(stat: &str, uptime: &str) -> Option<Duration> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    let started_ticks: f64 = after_name.split_whitespace().nth(19)?.parse().ok()?;
    let up_seconds: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    let age = up_seconds - started_ticks / TICKS_PER_SECOND;
    age.is_finite()
        .then(|| Duration::from_secs_f64(age.max(0.0)))
}

pub(super) fn parse_peak(status: &str) -> Option<u64> {
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?;
    let kilobytes: u64 = line.split_whitespace().next()?.parse().ok()?;
    kilobytes.checked_mul(1024)
}

#[cfg(unix)]
pub(crate) fn process_age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    age_from_proc(&stat, &uptime)
}

#[cfg(unix)]
pub(super) fn peak_working_set() -> Option<u64> {
    parse_peak(&std::fs::read_to_string("/proc/self/status").ok()?)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub private_bytes: Option<u64>,
    pub working_set_bytes: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
}

impl Memory {
    pub fn sample() -> Self {
        let sampled = crate::diag::probe().sample().ok();
        Self {
            private_bytes: sampled.map(|memory| memory.private_bytes),
            working_set_bytes: sampled.map(|memory| memory.resident_bytes),
            peak_working_set_bytes: peak(),
        }
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn parse_peak(status: &str) -> Option<u64> {
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?;
    let kilobytes: u64 = line.split_whitespace().next()?.parse().ok()?;
    kilobytes.checked_mul(1024)
}

#[cfg(target_os = "linux")]
fn peak() -> Option<u64> {
    parse_peak(&std::fs::read_to_string("/proc/self/status").ok()?)
}

#[cfg(windows)]
fn peak() -> Option<u64> {
    crate::windows_memory::peak_working_set()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn peak() -> Option<u64> {
    None
}

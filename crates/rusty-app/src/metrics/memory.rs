#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub private_bytes: Option<u64>,
    pub working_set_bytes: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
}

impl Memory {
    pub fn sample() -> Self {
        Self::default()
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn parse_peak(status: &str) -> Option<u64> {
    let _ = status;
    None
}

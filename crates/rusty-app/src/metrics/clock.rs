use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct ProcessClock {
    start: Instant,
}

impl ProcessClock {
    pub fn starting_at(start: Instant) -> Self {
        Self { start }
    }

    pub fn detect(main_started: Instant) -> Self {
        Self::starting_at(main_started)
    }

    pub fn start(&self) -> Instant {
        self.start
    }

    pub fn ms(&self, at: Instant) -> f64 {
        let _ = at;
        0.0
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn age_from_proc(stat: &str, uptime: &str) -> Option<Duration> {
    let _ = (stat, uptime);
    None
}

#[cfg(any(windows, test))]
pub fn age_from_filetimes(created: u64, now: u64) -> Option<Duration> {
    let _ = (created, now);
    None
}

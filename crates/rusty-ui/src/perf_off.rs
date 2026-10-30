pub use rusty_diag::perf::{Latency, UiFlag};

#[must_use]
pub fn is_recording() -> bool {
    false
}

pub fn begin(_kind: Latency) {}

pub fn ready(_kind: Latency) {}

pub fn cancel(_kind: Latency) {}

pub fn animating(_kind: Latency, _is_running: bool) {}

pub fn note(_flag: UiFlag) {}

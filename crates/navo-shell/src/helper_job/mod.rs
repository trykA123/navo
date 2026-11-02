#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(windows)]
pub use platform::{HelperJob, confine_helper};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperLimits {
    pub memory_bytes: usize,
}

#[cfg(test)]
#[cfg(windows)]
mod tests;

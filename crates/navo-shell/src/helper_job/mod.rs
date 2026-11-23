#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(windows)]
mod handle_win;

#[cfg(windows)]
pub use handle_win::{adopt_shared_file, share_read_handle};

#[cfg(windows)]
pub use platform::{HelperJob, confine_helper};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperLimits {
    pub memory_bytes: usize,
}

#[cfg(test)]
#[cfg(windows)]
mod tests;

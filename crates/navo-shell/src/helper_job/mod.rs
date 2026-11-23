#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(any(windows, test))]
mod cmdline;

#[cfg(any(windows, test))]
mod sandbox_policy;

#[cfg(windows)]
mod sandbox_win;

#[cfg(windows)]
mod handle_win;

#[cfg(windows)]
mod spawn_win;

#[cfg(windows)]
pub use spawn_win::{ConfinedChild, ConfinedProcess, spawn_confined};

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

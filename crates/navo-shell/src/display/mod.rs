#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{cursor_position, display_scale, display_under_cursor};

#[cfg(not(windows))]
#[must_use]
pub fn display_scale(_display: u64) -> Option<f32> {
    None
}

/// The monitor the mouse cursor is on, as the GPUI display id; `None` off Windows.
#[cfg(not(windows))]
#[must_use]
pub fn display_under_cursor() -> Option<u64> {
    None
}

/// The mouse cursor in GPUI's logical pixels; `None` off Windows.
#[cfg(not(windows))]
#[must_use]
pub fn cursor_position() -> Option<(i32, i32)> {
    None
}

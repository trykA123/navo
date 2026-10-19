#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::display_scale;

#[cfg(not(windows))]
#[must_use]
pub fn display_scale(_display: u64) -> Option<f32> {
    None
}

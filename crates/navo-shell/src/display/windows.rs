use windows_sys::Win32::Graphics::Gdi::HMONITOR;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

const DEFAULT_DPI: f32 = 96.0;

#[must_use]
pub fn display_scale(display: u64) -> Option<f32> {
    let monitor = display as usize as HMONITOR;
    let (mut x, mut y) = (0_u32, 0_u32);
    // SAFETY: GetDpiForMonitor only reads the handle and writes the two out values; a stale handle yields an error code.
    let result = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    (result >= 0 && x > 0).then(|| x as f32 / DEFAULT_DPI)
}

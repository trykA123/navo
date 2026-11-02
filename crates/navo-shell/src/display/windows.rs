use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

const DEFAULT_DPI: f32 = 96.0;

#[must_use]
pub fn display_scale(display: u64) -> Option<f32> {
    let monitor = display as usize as HMONITOR;
    let (mut x, mut y) = (0_u32, 0_u32);
    // SAFETY: GetDpiForMonitor only reads the handle and writes the two out values; a stale handle yields an error code.
    let result = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    (result >= 0 && x > 0).then(|| x as f32 / DEFAULT_DPI)
}

/// The monitor the mouse cursor is on, as the GPUI display id (its `HMONITOR`).
#[must_use]
pub fn display_under_cursor() -> Option<u64> {
    let mut point = POINT::default();
    // SAFETY: GetCursorPos writes one POINT through a valid pointer.
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    // SAFETY: MonitorFromPoint takes the point by value and has no other preconditions.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    (!monitor.is_null()).then_some(monitor as usize as u64)
}

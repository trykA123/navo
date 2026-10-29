use rusty_ui::window_view::RawWindowHandle;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, ShowWindow};

pub(super) fn set_shown(handle: Option<RawWindowHandle>, shown: bool) -> bool {
    let Some(RawWindowHandle::Win32(handle)) = handle else {
        tracing::warn!(shown, "the window has no Win32 handle to show or hide");
        return false;
    };
    let hwnd = handle.hwnd.get() as HWND;
    let command = if shown { SW_SHOW } else { SW_HIDE };
    // SAFETY: `hwnd` is the live main window this process created; ShowWindow only changes its visibility.
    unsafe { ShowWindow(hwnd, command) };
    true
}

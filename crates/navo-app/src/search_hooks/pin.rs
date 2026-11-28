//! Windows z-order pinning and owner fallback hooks for the find results window.

use std::sync::Arc;

use navo_ui::window_view::{RawWindowHandle, WindowOrder};

/// Returns a window-ordering hook that toggles the Win32 topmost flag without activating.
#[cfg(windows)]
pub(super) fn order() -> WindowOrder {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    Arc::new(|handle, on| {
        let Some(hwnd) = win32_hwnd(handle) else {
            tracing::warn!(on, "the find window has no Win32 handle to pin");
            return;
        };
        let insert_after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        // SAFETY: `hwnd` is the live find results window created by this process;
        // `SetWindowPos` only adjusts its z-order without moving, resizing or activating it.
        unsafe { SetWindowPos(hwnd, insert_after, 0, 0, 0, 0, flags) };
    })
}

/// Returns a no-op window-ordering hook on platforms without Win32 window pinning.
#[cfg(not(windows))]
pub(super) fn order() -> WindowOrder {
    Arc::new(|_, _| {})
}

/// Verifies that `find` is owned by `navo`, repairing owner and taskbar style when needed.
#[cfg(windows)]
pub(super) fn ensure_owner(find: RawWindowHandle, navo: RawWindowHandle) -> bool {
    use std::ptr::null_mut;

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindow, GetWindowLongPtrW, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
        WS_EX_APPWINDOW,
    };

    let (Some(find_hwnd), Some(navo_hwnd)) = (win32_hwnd(find), win32_hwnd(navo)) else {
        tracing::warn!("missing Win32 window handle while checking find window owner");
        return false;
    };
    if find_hwnd == navo_hwnd {
        tracing::warn!("find window handle matches its expected owner");
        return false;
    }
    // SAFETY: `find_hwnd` is a live window created by this process; `GetWindow` only reads its owner.
    let owner = unsafe { GetWindow(find_hwnd, GW_OWNER) };
    if owner == navo_hwnd {
        return false;
    }
    // SAFETY: `find_hwnd` and `navo_hwnd` are distinct live top-level windows created by this process;
    // setting `GWLP_HWNDPARENT` updates the owner relationship without changing parent/child styles.
    unsafe { SetWindowLongPtrW(find_hwnd, GWLP_HWNDPARENT, navo_hwnd as isize) };
    // SAFETY: `find_hwnd` is a live window created by this process; clearing `WS_EX_APPWINDOW` removes
    // any stray taskbar button and `SWP_FRAMECHANGED` flushes cached window style state.
    unsafe {
        let style = GetWindowLongPtrW(find_hwnd, GWL_EXSTYLE);
        let cleared = style & !(WS_EX_APPWINDOW as isize);
        if cleared != style {
            SetWindowLongPtrW(find_hwnd, GWL_EXSTYLE, cleared);
        }
        SetWindowPos(
            find_hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    true
}

/// Always reports no owner repair needed on platforms without Win32 owner handles.
#[cfg(not(windows))]
pub(super) fn ensure_owner(_find: RawWindowHandle, _navo: RawWindowHandle) -> bool {
    false
}

#[cfg(windows)]
fn win32_hwnd(handle: RawWindowHandle) -> Option<windows_sys::Win32::Foundation::HWND> {
    let RawWindowHandle::Win32(handle) = handle else {
        return None;
    };
    Some(handle.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

use std::ptr::{null, null_mut};

use rusty_core::placement::PhysicalRect;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, GetMonitorInfoW, HFONT, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromRect,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, IDC_APPSTARTING, LoadCursorW, MA_NOACTIVATE,
    NONCLIENTMETRICSW, RegisterClassExW, SPI_GETCLIENTAREAANIMATION, SPI_GETNONCLIENTMETRICS,
    SystemParametersInfoW, WM_MOUSEACTIVATE, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use super::canvas::{self, Look};
use super::timing::Motion;

const CLASS: &str = "FerriteSplash";
const BASE_DPI: f32 = 96.0;
const STATUS_SIZE: f32 = 12.0;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE as LRESULT;
    }
    // SAFETY: forwards the arguments the system passed to this window procedure.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub(super) fn open_window(origin: POINT, width: i32, height: i32) -> Option<HWND> {
    let class = wide(CLASS);
    let title = wide("Navo");
    // SAFETY: the class name and title outlive the calls; the window procedure is a valid static function.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(null());
        let description = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_APPSTARTING),
            lpszClassName: class.as_ptr(),
            ..WNDCLASSEXW::default()
        };
        RegisterClassExW(&raw const description);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            origin.x,
            origin.y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    (!hwnd.is_null()).then_some(hwnd)
}

pub(super) fn monitor(near: Option<PhysicalRect>) -> (RECT, f32) {
    // SAFETY: plain value arguments; MONITOR_DEFAULTTOPRIMARY always yields a monitor.
    let handle: HMONITOR = unsafe {
        match near {
            Some(rect) => {
                let rect = RECT {
                    left: rect.x,
                    top: rect.y,
                    right: rect.x.saturating_add(rect.width as i32),
                    bottom: rect.y.saturating_add(rect.height as i32),
                };
                MonitorFromRect(&raw const rect, MONITOR_DEFAULTTOPRIMARY)
            }
            None => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
        }
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    let (mut dpi, mut unused) = (0_u32, 0_u32);
    // SAFETY: `info` and the DPI outputs are live, writable values of the expected types.
    unsafe {
        GetMonitorInfoW(handle, &raw mut info);
        GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &raw mut dpi, &raw mut unused);
    }
    let scale = if dpi == 0 { 1.0 } else { dpi as f32 / BASE_DPI };
    (info.rcWork, scale)
}

pub(super) fn status_font(scale: f32) -> HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..NONCLIENTMETRICSW::default()
    };
    // SAFETY: `metrics` is a live NONCLIENTMETRICSW with its size set, as SPI_GETNONCLIENTMETRICS requires.
    unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            (&raw mut metrics).cast(),
            0,
        );
        let mut font = metrics.lfMessageFont;
        font.lfHeight = -((STATUS_SIZE * scale).round() as i32);
        CreateFontIndirectW(&raw const font)
    }
}

pub(super) fn theme() -> Look {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut value = 1_u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: the key and value names are NUL-terminated and `value` holds the four bytes a DWORD needs.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut value).cast(),
            &raw mut size,
        )
    };
    if status == ERROR_SUCCESS && value == 0 {
        canvas::DARK
    } else {
        canvas::LIGHT
    }
}

pub(super) fn motion() -> Motion {
    let mut enabled = 1_i32;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL into `enabled`.
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0)
    };
    if ok != 0 && enabled == 0 {
        Motion::Still
    } else {
        Motion::Animated
    }
}

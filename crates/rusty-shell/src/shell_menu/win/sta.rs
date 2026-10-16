use std::cell::RefCell;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::UI::Shell::{IContextMenu2, IContextMenu3};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostMessageW,
    QS_ALLINPUT, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_MENUSELECT, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use super::shell_error;
use crate::shell_menu::MenuError;

const CLASS: PCWSTR = w!("FerriteShellMenu");
pub(super) const WM_WAKE: u32 = WM_APP + 1;

pub(super) enum MenuHandler {
    Menu3(IContextMenu3),
    Menu2(IContextMenu2),
}

thread_local! {
    static ACTIVE: RefCell<Option<MenuHandler>> = const { RefCell::new(None) };
}

pub(super) struct ActiveMenu;

impl ActiveMenu {
    pub(super) fn set(handler: Option<MenuHandler>) -> Self {
        ACTIVE.with(|active| *active.borrow_mut() = handler);
        Self
    }
}

impl Drop for ActiveMenu {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.borrow_mut().take());
    }
}

pub(super) struct Ole;

impl Ole {
    pub(super) fn init() -> Result<Self, MenuError> {
        // SAFETY: first COM call on this thread; paired with OleUninitialize in Drop.
        unsafe { OleInitialize(None) }.map_err(shell_error("OleInitialize"))?;
        Ok(Self)
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        // SAFETY: OleInitialize succeeded on this thread and every COM object is gone.
        unsafe { OleUninitialize() };
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(result) = forward_menu_message(msg, wparam, lparam) {
        return result;
    }
    // SAFETY: forwards the arguments the system passed to this window procedure.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn forward_menu_message(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let is_menu_message = matches!(
        msg,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR | WM_MENUSELECT
    );
    if !is_menu_message {
        return None;
    }
    ACTIVE
        .try_with(|active| {
            let active = active.try_borrow().ok()?;
            let mut result = LRESULT(0);
            // SAFETY: a menu message delivered during TrackPopupMenuEx for this handler's menu.
            let handled = unsafe {
                match active.as_ref()? {
                    MenuHandler::Menu3(menu) => {
                        menu.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result))
                    }
                    MenuHandler::Menu2(menu) => menu.HandleMenuMsg(msg, wparam, lparam),
                }
            };
            handled.ok().map(|()| result)
        })
        .ok()
        .flatten()
}

pub(super) struct Window(HWND);

impl Window {
    fn register() -> Result<(), MenuError> {
        // SAFETY: a static class name and a valid window procedure.
        unsafe {
            let instance = GetModuleHandleW(None).map_err(shell_error("GetModuleHandleW"))?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassW(&class);
        }
        Ok(())
    }

    pub(super) fn message_only() -> Result<Self, MenuError> {
        Self::create(WINDOW_EX_STYLE(0), WINDOW_STYLE(0), Some(HWND_MESSAGE))
    }

    pub(super) fn popup() -> Result<Self, MenuError> {
        Self::create(WS_EX_TOOLWINDOW, WS_POPUP, None)
    }

    fn create(
        ex_style: WINDOW_EX_STYLE,
        style: WINDOW_STYLE,
        parent: Option<HWND>,
    ) -> Result<Self, MenuError> {
        Self::register()?;
        // SAFETY: the class was registered above with a valid window procedure.
        unsafe {
            let instance = GetModuleHandleW(None).map_err(shell_error("GetModuleHandleW"))?;
            CreateWindowExW(
                ex_style,
                CLASS,
                w!(""),
                style,
                0,
                0,
                0,
                0,
                parent,
                None,
                Some(instance.into()),
                None,
            )
            .map(Self)
            .map_err(shell_error("CreateWindowExW"))
        }
    }

    pub(super) fn hwnd(&self) -> HWND {
        self.0
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the window was created on this thread and is destroyed once.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

pub(super) fn wake(window: isize) -> bool {
    let hwnd = HWND(window as *mut _);
    // SAFETY: posting to a window handle is safe even if the window is gone.
    unsafe { PostMessageW(Some(hwnd), WM_WAKE, WPARAM(0), LPARAM(0)) }.is_ok()
}

pub(super) fn wait_message() -> bool {
    let mut msg = MSG::default();
    // SAFETY: standard message loop on the thread that owns the window.
    unsafe {
        if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
            return false;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    true
}

pub(super) fn pump_until(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: waits on this thread's own message queue only.
    unsafe { MsgWaitForMultipleObjectsEx(None, timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
    let mut msg = MSG::default();
    // SAFETY: dispatches messages queued for this thread's windows.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

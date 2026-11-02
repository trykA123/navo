use std::ffi::CString;

use windows::Win32::Foundation::{E_INVALIDARG, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
};
use windows::core::{HSTRING, PCSTR, PCWSTR};

use super::loaded::LoadedMenu;
use super::read::ID_FIRST;
use super::sta::{ActiveMenu, Window};
use super::{hwnd_of, shell_error};
use crate::shell_menu::verb_fallback::is_unknown_verb;
use crate::shell_menu::{InvokeOptions, MenuError, ScreenPoint, Show, WindowHandle};

const INVOKE: &str = "IContextMenu::InvokeCommand";
const CMIC_MASK_NOASYNC: u32 = 0x0000_0100;
const CMIC_MASK_UNICODE: u32 = 0x0000_4000;

pub(super) struct Target<'a> {
    pub command: u32,
    pub verb: Option<&'a str>,
}

pub(super) fn invoke(
    menu: &LoadedMenu,
    target: &Target<'_>,
    options: InvokeOptions,
) -> Result<(), MenuError> {
    let offset = menu.verbs().offset(target.command);
    if let Some(verb) = target.verb {
        match invoke_verb(menu, verb, options) {
            Ok(()) => return Ok(()),
            Err(error) if offset.is_some() && is_unknown_verb(error.code().0) => {}
            Err(error) => return Err(shell_error(INVOKE)(error)),
        }
    }
    let offset = offset.ok_or(MenuError::NotShellCommand)?;
    invoke_offset(menu, offset, options).map_err(shell_error(INVOKE))
}

fn invoke_verb(menu: &LoadedMenu, verb: &str, options: InvokeOptions) -> windows::core::Result<()> {
    let ansi = CString::new(verb).map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
    let wide = HSTRING::from(verb);
    let verbs = (PCSTR(ansi.as_ptr().cast()), PCWSTR(wide.as_ptr()));
    send(menu, verbs, options)
}

fn invoke_offset(
    menu: &LoadedMenu,
    offset: u32,
    options: InvokeOptions,
) -> windows::core::Result<()> {
    let id = offset as usize;
    send(
        menu,
        (PCSTR(id as *const u8), PCWSTR(id as *const u16)),
        options,
    )
}

fn send(
    menu: &LoadedMenu,
    (verb, verb_wide): (PCSTR, PCWSTR),
    options: InvokeOptions,
) -> windows::core::Result<()> {
    let point = options.point.unwrap_or_default();
    let point_mask = if options.point.is_some() {
        CMIC_MASK_PTINVOKE
    } else {
        0
    };
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: u32::try_from(size_of::<CMINVOKECOMMANDINFOEX>()).unwrap_or(0),
        fMask: CMIC_MASK_UNICODE | CMIC_MASK_NOASYNC | point_mask,
        hwnd: options.hwnd.map_or(HWND::default(), hwnd_of),
        lpVerb: verb,
        lpVerbW: verb_wide,
        nShow: show_code(options.show),
        ptInvoke: POINT {
            x: point.x,
            y: point.y,
        },
        ..Default::default()
    };
    // SAFETY: info and the strings it points to outlive the call; cbSize marks the EX layout.
    unsafe {
        menu.menu
            .InvokeCommand((&raw const info).cast::<CMINVOKECOMMANDINFO>())
    }
}

fn show_code(show: Show) -> i32 {
    match show {
        Show::Normal => SW_SHOWNORMAL.0,
        Show::Minimized => SW_SHOWMINNOACTIVE.0,
        Show::Maximized => SW_SHOWMAXIMIZED.0,
    }
}

pub(super) fn classic(
    menu: &LoadedMenu,
    hwnd: WindowHandle,
    point: ScreenPoint,
) -> Result<Option<u32>, MenuError> {
    let popup = Window::popup()?;
    let _active = ActiveMenu::set(menu.handler());
    // SAFETY: the menu and the popup window live on this thread for the whole modal loop.
    let chosen = unsafe {
        let _ = SetForegroundWindow(popup.hwnd());
        let flags = TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0;
        let chosen = TrackPopupMenuEx(menu.hmenu, flags, point.x, point.y, popup.hwnd(), None);
        let _ = PostMessageW(Some(popup.hwnd()), WM_NULL, WPARAM(0), LPARAM(0));
        chosen.0
    };
    let Some(command) = u32::try_from(chosen).ok().filter(|id| *id >= ID_FIRST) else {
        return Ok(None);
    };
    let options = InvokeOptions {
        hwnd: Some(hwnd),
        point: Some(point),
        show: Show::Normal,
    };
    invoke_offset(menu, command - ID_FIRST, options).map_err(shell_error(INVOKE))?;
    Ok(Some(command))
}

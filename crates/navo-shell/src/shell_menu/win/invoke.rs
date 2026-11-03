use std::ffi::CString;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{E_INVALIDARG, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Globalization::{CP_ACP, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte};
use windows::Win32::UI::Shell::{CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
};
use windows::core::{BOOL, HSTRING, PCSTR, PCWSTR};

use super::loaded::LoadedMenu;
use super::read::ID_FIRST;
use super::sta::{ActiveMenu, Window};
use super::{hwnd_of, shell_error};
use crate::shell_menu::invoke_plan::{Command, Plan, plan};
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
    let first = plan(&menu.paths, offset, target.verb)?;
    match send(menu, &first, options) {
        Err(error) if offset.is_some() && is_unknown_verb(error.code().0) => {
            let verb = target.verb.ok_or(shell_error(INVOKE)(error.clone()))?;
            let by_verb = plan(&menu.paths, None, Some(verb))?;
            send(menu, &by_verb, options).map_err(shell_error(INVOKE))
        }
        result => result.map_err(shell_error(INVOKE)),
    }
}

/// The strings `CMINVOKECOMMANDINFOEX` points to; they must outlive the call.
struct Strings {
    verb_ansi: Option<CString>,
    verb_wide: Option<HSTRING>,
    directory_ansi: Option<CString>,
    directory_wide: Option<HSTRING>,
}

impl Strings {
    fn new(plan: &Plan<'_>) -> windows::core::Result<Self> {
        let (verb_ansi, verb_wide) = match plan.command {
            Command::Verb(verb) => {
                let ansi =
                    CString::new(verb).map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
                (Some(ansi), Some(HSTRING::from(verb)))
            }
            Command::Offset(_) => (None, None),
        };
        let directory = plan.directory.as_deref();
        Ok(Self {
            verb_ansi,
            verb_wide,
            directory_ansi: directory.and_then(ansi_path),
            directory_wide: directory.map(|folder| HSTRING::from(folder.as_os_str())),
        })
    }

    fn verbs(&self, command: Command<'_>) -> (PCSTR, PCWSTR) {
        match command {
            Command::Offset(offset) => {
                let id = offset as usize;
                (PCSTR(id as *const u8), PCWSTR(id as *const u16))
            }
            Command::Verb(_) => (
                self.verb_ansi
                    .as_ref()
                    .map_or(PCSTR::null(), |ansi| PCSTR(ansi.as_ptr().cast())),
                self.verb_wide
                    .as_ref()
                    .map_or(PCWSTR::null(), |wide| PCWSTR(wide.as_ptr())),
            ),
        }
    }

    fn directories(&self) -> (PCSTR, PCWSTR) {
        (
            self.directory_ansi
                .as_ref()
                .map_or(PCSTR::null(), |ansi| PCSTR(ansi.as_ptr().cast())),
            self.directory_wide
                .as_ref()
                .map_or(PCWSTR::null(), |wide| PCWSTR(wide.as_ptr())),
        )
    }
}

/// The folder in the ANSI code page, or `None` when a character has no exact ANSI form.
fn ansi_path(folder: &Path) -> Option<CString> {
    let wide: Vec<u16> = folder.as_os_str().encode_wide().collect();
    let mut used_default = BOOL(0);
    // SAFETY: both calls read `wide` and write at most `buf.len()` bytes into `buf`.
    let bytes = unsafe {
        let flags = WC_NO_BEST_FIT_CHARS;
        let needed = WideCharToMultiByte(CP_ACP, flags, &wide, None, PCSTR::null(), None);
        let mut buf = vec![0u8; usize::try_from(needed).ok().filter(|n| *n > 0)?];
        let written = WideCharToMultiByte(
            CP_ACP,
            flags,
            &wide,
            Some(&mut buf),
            PCSTR::null(),
            Some(&raw mut used_default),
        );
        buf.truncate(usize::try_from(written).ok()?);
        buf
    };
    if used_default.as_bool() {
        return None;
    }
    CString::new(bytes).ok()
}

fn send(menu: &LoadedMenu, plan: &Plan<'_>, options: InvokeOptions) -> windows::core::Result<()> {
    let strings = Strings::new(plan)?;
    let (verb, verb_wide) = strings.verbs(plan.command);
    let (directory, directory_wide) = strings.directories();
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
        lpDirectory: directory,
        lpVerbW: verb_wide,
        lpDirectoryW: directory_wide,
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
    let plan = plan(&menu.paths, Some(command - ID_FIRST), None)?;
    send(menu, &plan, options).map_err(shell_error(INVOKE))?;
    Ok(Some(command))
}

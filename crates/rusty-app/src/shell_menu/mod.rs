use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use rusty_core::Location;
use rusty_core::shell_menu::{ItemId, ShellMenu, Submenu};
use rusty_shell::{
    InvokeOptions, MenuSession, Pending, ScreenPoint, ShellMenuService, Show, WindowHandle,
};
use rusty_ui::window_view::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};

const NOT_LOCAL: &str = "the Windows menu works only on local files";
const TOO_LONG: &str = "the Windows menu cannot open paths longer than 259 characters";
const RENAMED: &str = "the Windows menu cannot open names that Windows would change: \
     a trailing dot or space, or a device name such as NUL";
const SHELL_MAX: usize = 259;
const VERBATIM: &str = r"\\?\";
const VERBATIM_UNC: &str = r"\\?\UNC\";

pub fn provider() -> Option<Rc<dyn ShellMenuProvider>> {
    let service = ShellMenuService::start().ok()?;
    Some(Rc::new(Provider(service)))
}

struct Provider(ShellMenuService);

struct Session(MenuSession);

struct Unavailable {
    menu: ShellMenu,
    reason: String,
}

fn path_of(location: &Location) -> Result<PathBuf, String> {
    if !location.is_local() {
        return Err(NOT_LOCAL.to_owned());
    }
    rusty_fs::os_path(location)
        .map_err(|error| error.message())
        .and_then(shell_form)
}

fn shell_form(path: PathBuf) -> Result<PathBuf, String> {
    let plain = without_verbatim(path.into_os_string());
    let text = plain.to_string_lossy();
    if text.encode_utf16().count() > SHELL_MAX {
        return Err(TOO_LONG.to_owned());
    }
    if text.split(['\\', '/']).any(is_renamed_by_win32) {
        return Err(RENAMED.to_owned());
    }
    Ok(PathBuf::from(plain))
}

fn without_verbatim(path: OsString) -> OsString {
    let bytes = path.as_encoded_bytes();
    let starts_with = |head: &str| {
        bytes
            .get(..head.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(head.as_bytes()))
    };
    let (keep, cut) = if starts_with(VERBATIM_UNC) {
        (r"\\", VERBATIM_UNC.len())
    } else if starts_with(VERBATIM) {
        ("", VERBATIM.len())
    } else {
        return path;
    };
    // SAFETY: the cut falls right after an ASCII prefix of bytes from `as_encoded_bytes`.
    let rest = unsafe { OsStr::from_encoded_bytes_unchecked(&bytes[cut..]) };
    let mut plain = OsString::from(keep);
    plain.push(rest);
    plain
}

fn is_renamed_by_win32(part: &str) -> bool {
    if part.ends_with(['.', ' ']) {
        return true;
    }
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    match upper.as_bytes() {
        b"CON" | b"PRN" | b"AUX" | b"NUL" => true,
        [b'C', b'O', b'M', digit] | [b'L', b'P', b'T', digit] => (b'1'..=b'9').contains(digit),
        _ => false,
    }
}

fn reply<T: 'static, U>(pending: Pending<T>, map: impl Fn(T) -> U + 'static) -> Reply<U> {
    Box::new(move || {
        let answer = pending.wait_timeout(Duration::ZERO)?;
        Some(answer.map(&map).map_err(|error| error.to_string()))
    })
}

fn options(at: ScreenSpot) -> InvokeOptions {
    InvokeOptions {
        hwnd: at.window.map(WindowHandle),
        point: Some(ScreenPoint { x: at.x, y: at.y }),
        show: Show::Normal,
    }
}

impl ShellMenuProvider for Provider {
    fn open(&self, items: &[Location]) -> Reply<Box<dyn ShellMenuSession>> {
        let paths: Result<Vec<PathBuf>, String> = items.iter().map(path_of).collect();
        match paths {
            Ok(paths) => reply(self.0.open(paths), |session| {
                Box::new(Session(session)) as Box<dyn ShellMenuSession>
            }),
            Err(reason) => Box::new(move || {
                let unavailable = Unavailable {
                    menu: ShellMenu::default(),
                    reason: reason.clone(),
                };
                Some(Ok(Box::new(unavailable) as Box<dyn ShellMenuSession>))
            }),
        }
    }
}

impl ShellMenuSession for Session {
    fn menu(&self) -> &ShellMenu {
        self.0.menu()
    }

    fn fill(&self, path: &[usize]) -> Reply<Submenu> {
        reply(self.0.fill(path), |submenu| submenu)
    }

    fn invoke(&self, id: &ItemId, at: ScreenSpot) -> Reply<()> {
        reply(self.0.invoke(id, options(at)), |()| ())
    }

    fn classic(&self, at: ScreenSpot) -> Reply<()> {
        let window = WindowHandle(at.window.unwrap_or_default());
        let point = ScreenPoint { x: at.x, y: at.y };
        reply(self.0.classic(window, point), |_| ())
    }
}

impl ShellMenuSession for Unavailable {
    fn menu(&self) -> &ShellMenu {
        &self.menu
    }

    fn fill(&self, _: &[usize]) -> Reply<Submenu> {
        self.refuse()
    }

    fn invoke(&self, _: &ItemId, _: ScreenSpot) -> Reply<()> {
        self.refuse()
    }

    fn classic(&self, _: ScreenSpot) -> Reply<()> {
        self.refuse()
    }
}

impl Unavailable {
    fn refuse<T>(&self) -> Reply<T> {
        let reason = self.reason.clone();
        Box::new(move || Some(Err(reason.clone())))
    }
}

#[cfg(test)]
mod tests;

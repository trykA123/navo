#[cfg(any(windows, test))]
mod classify;
mod error;
#[cfg(any(windows, test))]
mod fill;
mod pending;
#[cfg(any(windows, test))]
mod pixels;
mod selection;
#[cfg(any(windows, test))]
mod station;
#[cfg(not(windows))]
mod stub;
#[cfg(any(windows, test))]
mod verb_fallback;
#[cfg(windows)]
mod win;

use std::path::PathBuf;

use rusty_core::shell_menu::{ItemId, ShellMenu, Submenu};

pub use error::MenuError;
pub use pending::Pending;
pub(crate) use selection::folder_key;

#[cfg(not(windows))]
use stub as platform;
#[cfg(windows)]
use win as platform;

pub const FILL_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(pub isize);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Show {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InvokeOptions {
    pub hwnd: Option<WindowHandle>,
    pub point: Option<ScreenPoint>,
    pub show: Show,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPaths {
    Items(Vec<PathBuf>),
    Background(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuSource {
    paths: MenuPaths,
    extended: bool,
}

impl MenuSource {
    fn checked(paths: MenuPaths, extended: bool) -> Result<Self, MenuError> {
        if let MenuPaths::Items(items) = &paths {
            selection::common_parent(items)?;
        }
        Ok(Self { paths, extended })
    }
}

pub struct ShellMenuService {
    inner: platform::Service,
}

impl ShellMenuService {
    pub fn start() -> Result<Self, MenuError> {
        platform::Service::start().map(|inner| Self { inner })
    }

    #[must_use]
    pub fn open(&self, paths: Vec<PathBuf>) -> Pending<MenuSession> {
        self.open_paths(MenuPaths::Items(paths), false)
    }

    /// The menu of the folder itself, as Explorer shows it for its empty space.
    #[must_use]
    pub fn open_background(&self, folder: PathBuf) -> Pending<MenuSession> {
        self.open_paths(MenuPaths::Background(folder), false)
    }

    /// `extended` adds the verbs Explorer shows while Shift is held (`CMF_EXTENDEDVERBS`).
    #[must_use]
    pub fn open_paths(&self, paths: MenuPaths, extended: bool) -> Pending<MenuSession> {
        match MenuSource::checked(paths, extended) {
            Ok(source) => self.inner.open(source),
            Err(error) => Pending::ready(Err(error)),
        }
    }

    /// The native menu without a session: one shell-thread request reads it and tracks it at `point`.
    /// Poll with `wait_timeout`, never block: the menu and its dialogs call into the UI window.
    #[must_use]
    pub fn classic_for(
        &self,
        paths: MenuPaths,
        extended: bool,
        (hwnd, point): (WindowHandle, ScreenPoint),
    ) -> Pending<Option<u32>> {
        match MenuSource::checked(paths, extended) {
            Ok(source) => self.inner.classic_for(source, hwnd, point),
            Err(error) => Pending::ready(Err(error)),
        }
    }
}

pub struct MenuSession {
    menu: ShellMenu,
    handle: platform::SessionHandle,
}

impl MenuSession {
    #[must_use]
    pub fn menu(&self) -> &ShellMenu {
        &self.menu
    }

    /// Answers within `FILL_BUDGET`, `Filled` or `Loading`; ask again later for the current state.
    #[must_use]
    pub fn fill(&self, path: &[usize]) -> Pending<Submenu> {
        self.handle.fill(path.to_vec())
    }

    /// Poll with `wait_timeout`, never block: dialogs owned by the UI window call into it.
    #[must_use]
    pub fn invoke(&self, id: &ItemId, options: InvokeOptions) -> Pending<()> {
        match id {
            ItemId::Shell { command, verb } => self.handle.invoke(*command, verb.clone(), options),
            ItemId::Ferrite(_) | ItemId::Action(_) => {
                Pending::ready(Err(MenuError::NotShellCommand))
            }
        }
    }

    /// Poll with `wait_timeout`, never block: the menu and its dialogs call into the UI window.
    #[must_use]
    pub fn classic(&self, hwnd: WindowHandle, point: ScreenPoint) -> Pending<Option<u32>> {
        self.handle.classic(hwnd, point)
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuSession, Pending, ShellMenuService};

    fn is_send_and_sync<T: Send + Sync>() {}

    #[test]
    fn the_service_and_sessions_cross_threads() {
        is_send_and_sync::<ShellMenuService>();
        is_send_and_sync::<MenuSession>();
    }

    #[test]
    fn a_pending_reply_can_move_to_another_thread() {
        fn is_send<T: Send>() {}
        is_send::<Pending<MenuSession>>();
    }
}

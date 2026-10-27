use rusty_core::Location;
use rusty_core::shell_menu::{ItemId, ShellMenu, Submenu};

/// Answers `None` while the shell is still working; never blocks.
pub type Reply<T> = Box<dyn Fn() -> Option<Result<T, String>>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenSpot {
    pub window: Option<isize>,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuTarget {
    Items(Vec<Location>),
    Background(Location),
}

impl MenuTarget {
    #[must_use]
    pub fn locations(&self) -> &[Location] {
        match self {
            Self::Items(items) => items,
            Self::Background(folder) => std::slice::from_ref(folder),
        }
    }
}

/// How the shell threads are doing: menus are read ahead only while `can_read_ahead`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellHealth {
    /// Changes each time a hung shell thread is replaced; a read sent before may never answer.
    pub replaced: u64,
    pub can_read_ahead: bool,
}

pub trait ShellMenuProvider {
    /// `extended` asks for the verbs Explorer adds while Shift is held.
    fn open(&self, target: &MenuTarget, extended: bool) -> Reply<Box<dyn ShellMenuSession>>;
    /// The native menu at `at`, read and shown in one go, for when no session could open.
    fn classic_for(&self, target: &MenuTarget, extended: bool, at: ScreenSpot) -> Reply<()>;
    fn health(&self) -> ShellHealth;
}

pub trait ShellMenuSession {
    fn menu(&self) -> &ShellMenu;
    fn fill(&self, path: &[usize]) -> Reply<Submenu>;
    /// Like `fill`, at the lowest priority; a later `fill` of the same path moves it to the front.
    fn prefill(&self, path: &[usize]) -> Reply<Submenu>;
    fn invoke(&self, id: &ItemId, at: ScreenSpot) -> Reply<()>;
    fn classic(&self, at: ScreenSpot) -> Reply<()>;
}

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

pub trait ShellMenuProvider {
    /// `extended` asks for the verbs Explorer adds while Shift is held.
    fn open(&self, target: &MenuTarget, extended: bool) -> Reply<Box<dyn ShellMenuSession>>;
    /// The native menu at `at`, read and shown in one go, for when no session could open.
    fn classic_for(&self, target: &MenuTarget, extended: bool, at: ScreenSpot) -> Reply<()>;
}

pub trait ShellMenuSession {
    fn menu(&self) -> &ShellMenu;
    fn fill(&self, path: &[usize]) -> Reply<Submenu>;
    fn invoke(&self, id: &ItemId, at: ScreenSpot) -> Reply<()>;
    fn classic(&self, at: ScreenSpot) -> Reply<()>;
}

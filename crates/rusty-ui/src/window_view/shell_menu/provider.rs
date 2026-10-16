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

pub trait ShellMenuProvider {
    fn open(&self, items: &[Location]) -> Reply<Box<dyn ShellMenuSession>>;
}

pub trait ShellMenuSession {
    fn menu(&self) -> &ShellMenu;
    fn fill(&self, path: &[usize]) -> Reply<Submenu>;
    fn invoke(&self, id: &ItemId, at: ScreenSpot) -> Reply<()>;
    fn classic(&self, at: ScreenSpot) -> Reply<()>;
}

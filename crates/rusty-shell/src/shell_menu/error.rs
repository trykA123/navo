use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MenuError {
    #[error("no items selected")]
    Empty,
    #[error("the selected items are in different folders")]
    DifferentFolders,
    #[error("cannot resolve {path}: {message}")]
    Path { path: PathBuf, message: String },
    #[error("{context}: {message}")]
    Shell {
        context: &'static str,
        message: String,
    },
    #[error("no submenu at {0:?}")]
    NoSubmenu(Vec<usize>),
    #[error("not a shell command")]
    NotShellCommand,
    #[error("the menu session is closed")]
    Closed,
    #[error("the shell menu thread stopped")]
    Stopped,
    #[error("shell menus are not supported on this platform")]
    Unsupported,
}

mod error;
mod id;
pub(crate) mod lexer;
mod parse;
pub(crate) mod print;

pub use error::{ParseError, ParseErrorKind};
pub use id::CommandId;

use crate::{Direction, Location, SpanMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PaneOpenTab { location: Option<Location> },
    PaneCloseTab { tab: Option<u32> },
    PaneNavigate { location: Location },
    PaneGoDrive { root: Location },
    PaneBack,
    PaneForward,
    PaneRefresh,
    PaneCursorMove { to: CursorMove },
    PaneActivateTab { tab: u32 },
    PaneScroll { to: u64 },
    SelectionToggle,
    SelectionToggleDown,
    SelectionRange { to: u64 },
    SelectionExtend { to: CursorMove, mode: SpanMode },
    SelectionExtension { mark: bool },
    SelectionRestore,
    SelectionAddPattern { pattern: String, folders: bool },
    SelectionRemovePattern { pattern: String, folders: bool },
    SelectionInvert,
    SelectionAll,
    SelectionClear,
    LayoutSplit { direction: Direction },
    LayoutClose,
    LayoutFocus { direction: Direction },
    WindowOpen { location: Option<Location> },
    WindowClose { window: Option<u64> },
    WindowFocus { window: u64 },
    WorkspaceSwitch { n: usize },
    WorkspaceNew { name: Option<String> },
    WorkspaceRename { n: usize, name: String },
    WorkspaceMove { n: usize, by: i8 },
    WorkspaceDelete { n: usize },
    WorkspaceRestore,
    WorkspaceSave,
    WorkspaceDiscard { n: usize },
    AppSaveSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    Row(u64),
}

impl CursorMove {
    const KEYWORDS: [(Self, &'static str); 6] = [
        (Self::Up, "up"),
        (Self::Down, "down"),
        (Self::PageUp, "page-up"),
        (Self::PageDown, "page-down"),
        (Self::First, "first"),
        (Self::Last, "last"),
    ];

    #[must_use]
    pub fn keyword(self) -> Option<&'static str> {
        Self::KEYWORDS
            .iter()
            .find(|(m, _)| *m == self)
            .map(|(_, k)| *k)
    }

    #[must_use]
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Self::KEYWORDS
            .iter()
            .find(|(_, k)| *k == keyword)
            .map(|(m, _)| *m)
    }
}

impl Command {
    #[must_use]
    pub fn id(&self) -> CommandId {
        use CommandId as Id;
        match self {
            Self::PaneOpenTab { .. } => Id::PaneOpenTab,
            Self::PaneCloseTab { .. } => Id::PaneCloseTab,
            Self::PaneNavigate { .. } => Id::PaneNavigate,
            Self::PaneGoDrive { .. } => Id::PaneGoDrive,
            Self::PaneBack => Id::PaneBack,
            Self::PaneForward => Id::PaneForward,
            Self::PaneRefresh => Id::PaneRefresh,
            Self::PaneCursorMove { .. } => Id::PaneCursorMove,
            Self::PaneActivateTab { .. } => Id::PaneActivateTab,
            Self::PaneScroll { .. } => Id::PaneScroll,
            Self::SelectionToggle => Id::SelectionToggle,
            Self::SelectionToggleDown => Id::SelectionToggleDown,
            Self::SelectionRange { .. } => Id::SelectionRange,
            Self::SelectionExtend { .. } => Id::SelectionExtend,
            Self::SelectionExtension { .. } => Id::SelectionExtension,
            Self::SelectionRestore => Id::SelectionRestore,
            Self::SelectionAddPattern { .. } => Id::SelectionAddPattern,
            Self::SelectionRemovePattern { .. } => Id::SelectionRemovePattern,
            Self::SelectionInvert => Id::SelectionInvert,
            Self::SelectionAll => Id::SelectionAll,
            Self::SelectionClear => Id::SelectionClear,
            Self::LayoutSplit { .. } => Id::LayoutSplit,
            Self::LayoutClose => Id::LayoutClose,
            Self::LayoutFocus { .. } => Id::LayoutFocus,
            Self::WindowOpen { .. } => Id::WindowOpen,
            Self::WindowClose { .. } => Id::WindowClose,
            Self::WindowFocus { .. } => Id::WindowFocus,
            Self::WorkspaceSwitch { .. } => Id::WorkspaceSwitch,
            Self::WorkspaceNew { .. } => Id::WorkspaceNew,
            Self::WorkspaceRename { .. } => Id::WorkspaceRename,
            Self::WorkspaceMove { .. } => Id::WorkspaceMove,
            Self::WorkspaceDelete { .. } => Id::WorkspaceDelete,
            Self::WorkspaceRestore => Id::WorkspaceRestore,
            Self::WorkspaceSave => Id::WorkspaceSave,
            Self::WorkspaceDiscard { .. } => Id::WorkspaceDiscard,
            Self::AppSaveSession => Id::AppSaveSession,
        }
    }

    pub fn parse(text: &str) -> Result<Self, ParseError> {
        parse::parse(text)
    }
}

impl std::str::FromStr for Command {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests;

mod error;
mod id;
mod lexer;
mod parse;
mod print;

pub use error::{ParseError, ParseErrorKind};
pub use id::CommandId;

use crate::{Direction, Location};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PaneOpenTab { location: Option<Location> },
    PaneCloseTab { tab: Option<u32> },
    PaneNavigate { location: Location },
    PaneBack,
    PaneForward,
    PaneCursorMove { to: CursorMove },
    SelectionToggle,
    SelectionToggleDown,
    SelectionRange { to: u64 },
    SelectionAddPattern { pattern: String },
    SelectionRemovePattern { pattern: String },
    SelectionInvert,
    SelectionAll,
    SelectionClear,
    LayoutSplit { direction: Direction },
    LayoutClose,
    LayoutFocus { direction: Direction },
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
            Self::PaneBack => Id::PaneBack,
            Self::PaneForward => Id::PaneForward,
            Self::PaneCursorMove { .. } => Id::PaneCursorMove,
            Self::SelectionToggle => Id::SelectionToggle,
            Self::SelectionToggleDown => Id::SelectionToggleDown,
            Self::SelectionRange { .. } => Id::SelectionRange,
            Self::SelectionAddPattern { .. } => Id::SelectionAddPattern,
            Self::SelectionRemovePattern { .. } => Id::SelectionRemovePattern,
            Self::SelectionInvert => Id::SelectionInvert,
            Self::SelectionAll => Id::SelectionAll,
            Self::SelectionClear => Id::SelectionClear,
            Self::LayoutSplit { .. } => Id::LayoutSplit,
            Self::LayoutClose => Id::LayoutClose,
            Self::LayoutFocus { .. } => Id::LayoutFocus,
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

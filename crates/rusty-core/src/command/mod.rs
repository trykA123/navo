mod error;
mod id;
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

impl Command {
    #[must_use]
    pub fn id(&self) -> CommandId {
        todo!()
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

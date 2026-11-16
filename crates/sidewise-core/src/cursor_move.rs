//! Cursor movement commands (up, down, page, first, last, row) and their text keywords.
//! Shared by the engine and the shell to describe where the cursor goes.

/// A cursor movement: one step, one page, a list end, or a given row.
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

    /// Returns the text keyword of a named move, or `None` for `Row`.
    #[must_use]
    pub fn keyword(self) -> Option<&'static str> {
        Self::KEYWORDS
            .iter()
            .find(|(m, _)| *m == self)
            .map(|(_, k)| *k)
    }

    /// Parses a keyword such as "page-up" into a move, or `None` if unknown.
    #[must_use]
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Self::KEYWORDS
            .iter()
            .find(|(_, k)| *k == keyword)
            .map(|(m, _)| *m)
    }
}

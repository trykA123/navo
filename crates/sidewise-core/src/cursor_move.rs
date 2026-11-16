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

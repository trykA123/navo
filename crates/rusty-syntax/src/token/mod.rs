use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Constant,
    Attribute,
    Tag,
    Property,
    Variable,
    Comment,
    String,
    Number,
    Operator,
    Delimiter,
}

impl TokenKind {
    #[must_use]
    pub fn highlight_name(self) -> &'static str {
        ""
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub range: Range<usize>,
    pub kind: TokenKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineState {
    #[default]
    Normal,
    Block(u8),
    Delimited(u8),
}

#[cfg(test)]
mod tests;

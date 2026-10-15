use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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
        match self {
            Self::Keyword => "keyword",
            Self::Type => "type",
            Self::Function => "function",
            Self::Constant => "constant",
            Self::Attribute => "attribute",
            Self::Tag => "tag",
            Self::Property => "property",
            Self::Variable => "variable.special",
            Self::Comment => "comment",
            Self::String => "string",
            Self::Number => "number",
            Self::Operator => "operator",
            Self::Delimiter => "punctuation.delimiter",
        }
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

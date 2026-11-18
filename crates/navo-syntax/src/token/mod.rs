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
    Block(u16),
    Delimited(u16),
}

/// Where a chunk of a line stops, as the lexer itself saw it (the chunk API, `Lexer::chunk`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Inside {
    /// Ordinary code: the next chunk starts a fresh token.
    #[default]
    Code,
    /// In a line comment that runs to the end of the line.
    LineComment,
    /// In code, where the chunk end may have cut comment or string opener `index`: the last
    /// `ChunkState::partial` bytes of the chunk are held back and lexed again with the next chunk.
    Opening(u16),
    /// In block comment `index`; `partial` bytes of its closer may end the chunk.
    Block(u16),
    /// In delimiter `index` (a string); `escaped` when the last char was an escape char.
    Delimited { index: u16, escaped: bool },
}

/// The state a chunk hands to the next chunk of the same line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkState {
    pub inside: Inside,
    /// The chunk ended inside a word, so the next chunk's first byte is not a word start.
    pub mid_word: bool,
    /// Bytes of the chunk end that the state above holds back: the start of a closer (inside a
    /// block or string) or of an opener (`Inside::Opening`). The next chunk lexes them again, in
    /// that state, in front of its own bytes. At most `MAX_PATTERN_BYTES - 1`.
    pub partial: u16,
}

impl ChunkState {
    /// The state a line starts in when it carries `state` over from the line before.
    #[must_use]
    pub fn from_line(state: LineState) -> Self {
        let inside = match state {
            LineState::Normal => Inside::Code,
            LineState::Block(index) => Inside::Block(index),
            LineState::Delimited(index) => Inside::Delimited {
                index,
                escaped: false,
            },
        };
        Self {
            inside,
            ..Self::default()
        }
    }

    /// The state the next line starts in, once this chunk ended its line.
    #[must_use]
    pub fn line_state(self) -> LineState {
        match self.inside {
            Inside::Code | Inside::LineComment | Inside::Opening(_) => LineState::Normal,
            Inside::Block(index) => LineState::Block(index),
            Inside::Delimited { index, .. } => LineState::Delimited(index),
        }
    }
}

#[cfg(test)]
mod tests;

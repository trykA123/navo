use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: u64,
    pub column: u64,
}

impl ParseError {
    #[must_use]
    pub fn new(message: impl Into<String>, line: u64, column: u64) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }

    #[must_use]
    pub fn at_offset(message: impl Into<String>, bytes: &[u8], offset: usize) -> Self {
        let (line, column) = line_column(bytes, offset);
        Self::new(message, line, column)
    }
}

#[must_use]
pub fn line_column(bytes: &[u8], offset: usize) -> (u64, u64) {
    let before = bytes.get(..offset.min(bytes.len())).unwrap_or_default();
    let line = before.iter().filter(|byte| **byte == b'\n').count() as u64 + 1;
    let start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    let column = String::from_utf8_lossy(before.get(start..).unwrap_or_default())
        .chars()
        .count() as u64
        + 1;
    (line, column)
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for ParseError {}

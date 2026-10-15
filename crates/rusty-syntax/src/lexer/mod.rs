use crate::{Language, LineState, Token};

#[must_use]
pub fn lex_line(_lang: &Language, _line: &str, _state: LineState) -> (Vec<Token>, LineState) {
    (Vec::new(), LineState::Normal)
}

#[cfg(test)]
mod tests;

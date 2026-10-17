use std::fmt;

use super::{Keymap, KeymapError};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Overlaid {
    pub keymap: Keymap,
    pub warnings: Vec<KeymapWarning>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeymapWarning {
    pub line: usize,
    pub key: String,
    pub context: String,
    pub kept: String,
    pub dropped: String,
}

impl fmt::Display for KeymapWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            line,
            key,
            context,
            kept,
            dropped,
        } = self;
        write!(
            formatter,
            "line {line}: `{key}` now runs `{kept}` instead of `{dropped}` in `{context}`"
        )
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UserKeymapError {
    pub line: Option<usize>,
    pub error: KeymapError,
}

impl fmt::Display for UserKeymapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "line {line}: {}", self.error),
            None => write!(formatter, "{}", self.error),
        }
    }
}

impl std::error::Error for UserKeymapError {}

pub(super) fn overlay(_base: &Keymap, _text: &str) -> Result<Overlaid, UserKeymapError> {
    todo!("P20 K4")
}

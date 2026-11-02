use std::borrow::Cow;

use crate::model::CaseRule;

use super::side::LineEnding;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Whitespace {
    #[default]
    Keep,
    All,
    Changes,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub whitespace: Whitespace,
    pub ignore_line_endings: bool,
    pub case: CaseRule,
    pub ignore_bom: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            whitespace: Whitespace::Keep,
            ignore_line_endings: false,
            case: CaseRule::Sensitive,
            ignore_bom: false,
        }
    }
}

#[must_use]
pub fn normalise(text: &str, ending: LineEnding, bom: bool, options: &Options) -> String {
    let mut key = String::with_capacity(text.len() + 2);
    if bom && !options.ignore_bom {
        key.push('\u{feff}');
    }
    let text = fold_case(text, options.case);
    match options.whitespace {
        Whitespace::Keep => key.push_str(&text),
        Whitespace::All => key.extend(text.chars().filter(|character| !character.is_whitespace())),
        Whitespace::Changes => key.push_str(&collapse(&text)),
        Whitespace::Trailing => key.push_str(text.trim_end()),
    }
    if !options.ignore_line_endings {
        key.push_str(ending.as_str());
    }
    key
}

fn fold_case(text: &str, case: CaseRule) -> Cow<'_, str> {
    match case {
        CaseRule::Sensitive => Cow::Borrowed(text),
        CaseRule::Insensitive => Cow::Owned(text.to_uppercase().to_lowercase()),
    }
}

fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending = true;
            continue;
        }
        if pending {
            out.push(' ');
            pending = false;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests;

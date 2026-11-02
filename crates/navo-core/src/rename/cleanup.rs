use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cleanup {
    pub trim: bool,
    pub collapse: bool,
    pub remove_accents: bool,
    pub spaces_to_underscore: bool,
}

impl Cleanup {
    pub(super) fn apply(self, text: &str) -> String {
        let mut text = text.to_owned();
        if self.remove_accents {
            text = text.chars().map(strip_marks).collect();
        }
        if self.collapse {
            text = collapse(&text);
        }
        if self.trim {
            text = text.trim().to_owned();
        }
        if self.spaces_to_underscore {
            text = text.replace(' ', "_");
        }
        text
    }
}

fn strip_marks(c: char) -> String {
    let decomposed: Vec<char> = std::iter::once(c).nfd().collect();
    if decomposed.iter().any(|d| is_combining_mark(*d)) {
        decomposed
            .into_iter()
            .filter(|d| !is_combining_mark(*d))
            .nfc()
            .collect()
    } else {
        c.to_string()
    }
}

fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = None;
    for c in text.chars() {
        let repeat = matches!(c, ' ' | '_' | '-' | '.') && last == Some(c);
        if !repeat {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

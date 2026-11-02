use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameCase {
    #[default]
    Unchanged,
    Lower,
    Upper,
    FirstLetter,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtCase {
    #[default]
    Unchanged,
    Lower,
    Upper,
}

impl NameCase {
    pub(super) fn apply(self, text: &str) -> String {
        match self {
            Self::Unchanged => text.to_owned(),
            Self::Lower => text.to_lowercase(),
            Self::Upper => text.to_uppercase(),
            Self::FirstLetter => capitalise(text),
            Self::Title => title(text),
        }
    }
}

impl ExtCase {
    pub(super) fn apply(self, text: &str) -> String {
        match self {
            Self::Unchanged => text.to_owned(),
            Self::Lower => text.to_lowercase(),
            Self::Upper => text.to_uppercase(),
        }
    }
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
    })
}

fn title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if matches!(ch, ' ' | '_' | '-' | '.') {
            out.push_str(&capitalise(&word));
            word.clear();
            out.push(ch);
        } else {
            word.push(ch);
        }
    }
    out.push_str(&capitalise(&word));
    out
}

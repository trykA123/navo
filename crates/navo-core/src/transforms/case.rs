//! Case conversion for editor lines, built on `NameCase` and word splitting.

use crate::rename::NameCase;
use crate::transforms::split_lines;

/// The case styles `convert_case` supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCase {
    /// All lowercase.
    Lower,
    /// All uppercase.
    Upper,
    /// Each word starts with an uppercase letter.
    Title,
    /// Only the first letter is uppercase.
    Sentence,
    /// Words lowercased and joined with underscores.
    Snake,
    /// First word lowercased, later words capitalised, no separator.
    Camel,
    /// Every word capitalised, no separator.
    Pascal,
    /// Words lowercased and joined with hyphens.
    Kebab,
}

/// Converts each line of `text` to `case`, keeping leading and trailing blanks.
pub fn convert_case(text: &str, case: TextCase) -> String {
    let parsed = split_lines(text);
    let converted: Vec<String> = parsed
        .lines
        .iter()
        .map(|line| convert_line(line, case))
        .collect();
    let lines: Vec<&str> = converted.iter().map(String::as_str).collect();
    parsed.join(&lines)
}

/// Converts one line's core text, restoring its leading and trailing spaces and tabs.
fn convert_line(line: &str, case: TextCase) -> String {
    let trimmed_start = line.trim_start_matches([' ', '\t']);
    let lead = &line[..line.len() - trimmed_start.len()];
    let core = trimmed_start.trim_end_matches([' ', '\t']);
    let trail = &trimmed_start[core.len()..];
    let converted = match case {
        TextCase::Lower => NameCase::Lower.apply(core),
        TextCase::Upper => NameCase::Upper.apply(core),
        TextCase::Title => NameCase::Title.apply(core),
        TextCase::Sentence => NameCase::FirstLetter.apply(core),
        TextCase::Snake => snake(&words(core)),
        TextCase::Kebab => kebab(&words(core)),
        TextCase::Camel => camel(&words(core)),
        TextCase::Pascal => pascal(&words(core)),
    };
    let mut out = String::with_capacity(line.len());
    out.push_str(lead);
    out.push_str(&converted);
    out.push_str(trail);
    out
}

/// Splits `part` into words on separators and case boundaries.
fn words(part: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut start = 0;
    let mut previous = None;
    let mut chars = part.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if is_separator(ch) {
            push_word(&mut words, part, start, at);
            start = at + ch.len_utf8();
            previous = None;
        } else {
            let next = chars.peek().map(|(_, next)| *next);
            if is_boundary(previous, ch, next) {
                push_word(&mut words, part, start, at);
                start = at;
            }
            previous = Some(ch);
        }
    }
    push_word(&mut words, part, start, part.len());
    words
}

/// Pushes `part[start..end]` as a word when the range is not empty.
fn push_word(words: &mut Vec<String>, part: &str, start: usize, end: usize) {
    if start < end {
        words.push(part[start..end].to_owned());
    }
}

/// Returns true when `current` starts a new word after `previous` and before `next`.
fn is_boundary(previous: Option<char>, current: char, next: Option<char>) -> bool {
    if !current.is_uppercase() {
        return false;
    }
    match previous {
        Some(previous) if previous.is_lowercase() || previous.is_ascii_digit() => true,
        Some(previous) if previous.is_uppercase() => next.is_some_and(char::is_lowercase),
        _ => false,
    }
}

/// Returns true for the separators dropped between words.
fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '_' | '-' | '.')
}

/// Lowercases `words` and joins them with underscores.
fn snake(words: &[String]) -> String {
    join_lower(words, '_')
}

/// Lowercases `words` and joins them with hyphens.
fn kebab(words: &[String]) -> String {
    join_lower(words, '-')
}

/// Lowercases `words` and joins them with `separator`.
fn join_lower(words: &[String], separator: char) -> String {
    let mut out = String::new();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            out.push(separator);
        }
        out.push_str(&word.to_lowercase());
    }
    out
}

/// Lowercases the first word and capitalises the rest, joined without a separator.
fn camel(words: &[String]) -> String {
    let mut out = String::new();
    for (index, word) in words.iter().enumerate() {
        if index == 0 {
            out.push_str(&word.to_lowercase());
        } else {
            out.push_str(&NameCase::FirstLetter.apply(word));
        }
    }
    out
}

/// Capitalises every word and joins them without a separator.
fn pascal(words: &[String]) -> String {
    let mut out = String::new();
    for word in words {
        out.push_str(&NameCase::FirstLetter.apply(word));
    }
    out
}

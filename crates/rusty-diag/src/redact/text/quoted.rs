use crate::is_redacted_path;
use crate::redact::names::{Replacement, replace, token_len};

const PAIRS: [(char, char); 4] = [
    ('\'', '\''),
    ('"', '"'),
    ('\u{201c}', '\u{201d}'),
    ('\u{2018}', '\u{2019}'),
];

pub(super) fn hide_quoted(text: &str, replacements: &[Replacement]) -> String {
    let mut output = String::with_capacity(text.len());
    let mut before: Option<char> = None;
    let mut rest = text;
    while let Some(open) = rest.chars().next() {
        let after_open = &rest[open.len_utf8()..];
        let closing = PAIRS
            .iter()
            .find(|(o, _)| *o == open)
            .filter(|_| before.is_none_or(|ch| !ch.is_alphanumeric()))
            .and_then(|(_, close)| closing_quote(after_open, *close).map(|end| (*close, end)));
        match closing {
            Some((close, end)) => {
                output.push(open);
                output.push_str(&hidden(&after_open[..end], replacements));
                output.push(close);
                before = Some(close);
                rest = &after_open[end + close.len_utf8()..];
            }
            None if opens_unclosed(open, before, after_open) => {
                output.push(open);
                output.push_str(&hidden(after_open, replacements));
                rest = "";
            }
            None => {
                output.push(open);
                before = Some(open);
                rest = after_open;
            }
        }
    }
    output
}

fn opens_unclosed(open: char, before: Option<char>, after_open: &str) -> bool {
    PAIRS.iter().any(|(o, _)| *o == open)
        && before.is_none_or(|ch| ch.is_whitespace() || matches!(ch, '=' | ':'))
        && !after_open.trim().is_empty()
}

fn closing_quote(text: &str, close: char) -> Option<usize> {
    text.char_indices()
        .filter(|&(_, ch)| ch == close)
        .map(|(index, _)| index)
        .find(|&index| {
            text[index + close.len_utf8()..]
                .chars()
                .next()
                .is_none_or(|ch| !ch.is_alphanumeric())
        })
        .filter(|&index| index > 0)
}

fn hidden(fragment: &str, replacements: &[Replacement]) -> String {
    let replaced = replace(fragment, replacements);
    if is_redacted_path(&replaced) || is_plain_token(&replaced) {
        replaced
    } else {
        format!("<text:{} chars>", fragment.chars().count())
    }
}

fn is_plain_token(fragment: &str) -> bool {
    token_len(fragment) == Some(fragment.len())
}

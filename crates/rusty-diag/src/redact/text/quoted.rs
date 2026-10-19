use crate::is_redacted_path;
use crate::redact::names::{Replacement, replace, token_len};

const PAIRS: [(char, char); 4] = [
    ('\'', '\''),
    ('"', '"'),
    ('\u{201c}', '\u{201d}'),
    ('\u{2018}', '\u{2019}'),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Quotes {
    Prose,
    Code,
}

pub(super) fn hide_quoted(text: &str, replacements: &[Replacement], quotes: Quotes) -> String {
    let closers = Closers::new(text, quotes);
    let mut output = String::with_capacity(text.len());
    let mut before: Option<char> = None;
    let mut index = 0;
    while let Some(open) = text[index..].chars().next() {
        let body = index + open.len_utf8();
        let lifetime = quotes == Quotes::Code && is_lifetime(text, index);
        let close = PAIRS
            .iter()
            .position(|(o, _)| *o == open)
            .filter(|_| before.is_none_or(|ch| !ch.is_alphanumeric()))
            .filter(|_| !lifetime);
        match close.and_then(|kind| closers.after(kind, body)) {
            Some((close, end)) => {
                output.push(open);
                output.push_str(&hidden(&text[body..end], replacements));
                output.push(close);
                before = Some(close);
                index = end + close.len_utf8();
            }
            None if !lifetime && opens_unclosed(open, before, &text[body..]) => {
                output.push(open);
                output.push_str(&hidden(&text[body..], replacements));
                index = text.len();
            }
            None => {
                output.push(open);
                before = Some(open);
                index = body;
            }
        }
    }
    output
}

struct Closers {
    positions: Vec<Vec<usize>>,
}

impl Closers {
    fn new(text: &str, quotes: Quotes) -> Self {
        let mut positions = vec![Vec::new(); PAIRS.len()];
        for (index, ch) in text.char_indices() {
            if quotes == Quotes::Code && is_lifetime(text, index) {
                continue;
            }
            let followed_ok = text[index + ch.len_utf8()..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric());
            if !followed_ok {
                continue;
            }
            PAIRS
                .iter()
                .zip(positions.iter_mut())
                .filter(|((_, close), _)| *close == ch)
                .for_each(|(_, list)| list.push(index));
        }
        Self { positions }
    }

    fn after(&self, kind: usize, body: usize) -> Option<(char, usize)> {
        let list = self.positions.get(kind)?;
        let first = list.partition_point(|&position| position <= body);
        let (_, close) = PAIRS.get(kind)?;
        list.get(first).map(|&end| (*close, end))
    }
}

fn is_lifetime(text: &str, index: usize) -> bool {
    let Some(name) = text[index..].strip_prefix('\'') else {
        return false;
    };
    let end = name
        .find(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .unwrap_or(name.len());
    let is_name = end > 0 && !name.starts_with(|ch: char| ch.is_ascii_digit());
    is_name
        && !name[end..].starts_with('\'')
        && text[..index]
            .trim_end_matches(' ')
            .ends_with(['<', '&', '+', ',', '('])
}

fn opens_unclosed(open: char, before: Option<char>, after_open: &str) -> bool {
    PAIRS.iter().any(|(o, _)| *o == open)
        && before.is_none_or(|ch| ch.is_whitespace() || matches!(ch, '=' | ':'))
        && !after_open.trim().is_empty()
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

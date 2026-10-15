use crate::is_redacted_path;
use crate::redact::names::{Replacement, replace, token_len};

const PAIRS: [(char, char); 4] = [
    ('\'', '\''),
    ('"', '"'),
    ('\u{201c}', '\u{201d}'),
    ('\u{2018}', '\u{2019}'),
];

pub(super) fn hide_quoted(text: &str, replacements: &[Replacement]) -> String {
    let closers = Closers::new(text);
    let mut output = String::with_capacity(text.len());
    let mut before: Option<char> = None;
    let mut index = 0;
    while let Some(open) = text[index..].chars().next() {
        let body = index + open.len_utf8();
        let close = PAIRS
            .iter()
            .position(|(o, _)| *o == open)
            .filter(|_| before.is_none_or(|ch| !ch.is_alphanumeric()));
        match close.and_then(|kind| closers.after(kind, body)) {
            Some((close, end)) => {
                output.push(open);
                output.push_str(&hidden(&text[body..end], replacements));
                output.push(close);
                before = Some(close);
                index = end + close.len_utf8();
            }
            None if opens_unclosed(open, before, &text[body..]) => {
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
    fn new(text: &str) -> Self {
        let mut positions = vec![Vec::new(); PAIRS.len()];
        for (index, ch) in text.char_indices() {
            let followed_ok = text[index + ch.len_utf8()..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric());
            if !followed_ok {
                continue;
            }
            for (kind, (_, close)) in PAIRS.iter().enumerate() {
                if *close == ch {
                    positions[kind].push(index);
                }
            }
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

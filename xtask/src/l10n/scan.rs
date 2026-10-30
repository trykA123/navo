use super::tokens::{Spanned, Token, tokenize};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Scan {
    pub calls: Vec<Found>,
    pub strings: Vec<Found>,
}

pub(super) fn scan(source: &str) -> Scan {
    let tokens = live_tokens(tokenize(source));
    Scan {
        calls: calls(&tokens),
        strings: strings(&tokens),
    }
}

/// Counted by the ratchet: starts with an ASCII capital and has two or more whitespace-separated words.
pub(super) fn is_capitalised_multi_word(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_uppercase()) && text.split_whitespace().nth(1).is_some()
}

fn strings(tokens: &[Spanned]) -> Vec<Found> {
    tokens
        .iter()
        .filter_map(|spanned| match &spanned.token {
            Token::Str(text) => Some(Found {
                text: text.clone(),
                line: spanned.line,
            }),
            _ => None,
        })
        .collect()
}

fn calls(tokens: &[Spanned]) -> Vec<Found> {
    let punct = |index: usize, wanted: char| matches!(tokens.get(index), Some(s) if s.token == Token::Punct(wanted));
    let mut found = Vec::new();
    for (index, spanned) in tokens.iter().enumerate() {
        let Token::Ident(name) = &spanned.token else {
            continue;
        };
        let open = match name.as_str() {
            "t" if punct(index + 1, '!') => index + 2,
            "tr" | "tr_args" => index + 1,
            _ => continue,
        };
        if let (
            true,
            Some(Spanned {
                token: Token::Str(id),
                line,
            }),
        ) = (punct(open, '('), tokens.get(open + 1))
        {
            found.push(Found {
                text: id.clone(),
                line: *line,
            });
        }
    }
    found
}

pub(crate) fn live_tokens(tokens: Vec<Spanned>) -> Vec<Spanned> {
    let mut live = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        match attribute_end(&tokens, index) {
            Some(end) if is_test_attribute(&tokens[index..end]) => index = item_end(&tokens, end),
            Some(end) => index = end,
            None => {
                live.push(tokens[index].clone());
                index += 1;
            }
        }
    }
    live
}

fn attribute_end(tokens: &[Spanned], start: usize) -> Option<usize> {
    let is = |index: usize, wanted: char| matches!(tokens.get(index), Some(s) if s.token == Token::Punct(wanted));
    if !is(start, '#') {
        return None;
    }
    let open = if is(start + 1, '!') {
        start + 2
    } else {
        start + 1
    };
    if !is(open, '[') {
        return None;
    }
    let mut depth = 0usize;
    for (offset, spanned) in tokens[open..].iter().enumerate() {
        match spanned.token {
            Token::Punct('[') => depth += 1,
            Token::Punct(']') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    Some(tokens.len())
}

fn is_test_attribute(attribute: &[Spanned]) -> bool {
    let compact: String = attribute
        .iter()
        .filter_map(|spanned| match &spanned.token {
            Token::Ident(name) => Some(name.clone()),
            Token::Punct(c) => Some(c.to_string()),
            Token::Str(_) => None,
        })
        .collect();
    compact == "#[cfg(test)]" || compact == "#[test]"
}

fn item_end(tokens: &[Spanned], start: usize) -> usize {
    let mut depth = 0usize;
    for (index, spanned) in tokens.iter().enumerate().skip(start) {
        match spanned.token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') if depth == 0 => return index,
            Token::Punct(')' | ']') => depth -= 1,
            Token::Punct('}') => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            Token::Punct(';' | ',') if depth == 0 => return index + 1,
            _ => {}
        }
    }
    tokens.len()
}

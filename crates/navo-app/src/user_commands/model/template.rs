//! Splits a command value into literal text and placeholders.

use std::iter::Peekable;
use std::str::Chars;

use super::{Part, Placeholder, Token};

pub(super) fn template(text: &str) -> Result<Token, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                let placeholder = placeholder(&mut chars)?;
                flush(&mut parts, &mut literal);
                parts.push(Part::Placeholder(placeholder));
            }
            '}' => {
                return Err("a lone `}` is not allowed; write `}}` for a literal brace".to_owned());
            }
            other => literal.push(other),
        }
    }
    flush(&mut parts, &mut literal);
    let whole = matches!(parts.as_slice(), [Part::Placeholder(Placeholder::Marked)]);
    let marked = parts
        .iter()
        .any(|part| matches!(part, Part::Placeholder(Placeholder::Marked)));
    if marked && !whole {
        return Err("`{marked}` must be a whole argument".to_owned());
    }
    Ok(Token { parts })
}

pub(super) fn first_placeholder(token: &Token) -> Option<Placeholder> {
    token.parts.iter().find_map(|part| match part {
        Part::Placeholder(placeholder) => Some(*placeholder),
        Part::Literal(_) => None,
    })
}

pub(super) fn literal_text(token: &Token) -> String {
    token
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Literal(text) => Some(text.as_str()),
            Part::Placeholder(_) => None,
        })
        .collect()
}

fn placeholder(chars: &mut Peekable<Chars<'_>>) -> Result<Placeholder, String> {
    let mut name = String::new();
    for next in chars.by_ref() {
        if next == '}' {
            return Placeholder::named(&name)
                .ok_or_else(|| format!("`{{{name}}}` is not a placeholder"));
        }
        name.push(next);
    }
    Err("a `{` is not closed; write `{{` for a literal brace".to_owned())
}

fn flush(parts: &mut Vec<Part>, literal: &mut String) {
    if !literal.is_empty() {
        parts.push(Part::Literal(std::mem::take(literal)));
    }
}

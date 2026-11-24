//! Tokenizes find query text and applies the token kinds that carry a typed value.

use std::iter::Peekable;
use std::ops::Range;
use std::str::CharIndices;

use super::{FindQuery, TokenKind};
use crate::Pattern;

/// How find decodes a file while searching it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Auto,
    Ansi,
    Dos,
    Utf8,
    Utf16,
}

const NAME_HELP: &str = "name: use masks like *.rs;!test* or /regex/";
const DEPTH_HELP: &str = "depth: use a number from 0 to 64";
const HEX_HELP: &str = "hex: use pairs like 4D 5A, up to 256 bytes";
const ENC_HELP: &str = "enc: use auto, ansi, dos, utf8 or utf16";

/// Applies one of the typed token kinds; other kinds are ignored.
pub(super) fn apply(query: &mut FindQuery, kind: TokenKind, value: &str) -> Result<(), String> {
    match kind {
        TokenKind::Name => name(query, value),
        TokenKind::Depth => depth(query, value),
        TokenKind::Word => on_off(value, "word", &mut query.word),
        TokenKind::Not => on_off(value, "not", &mut query.invert),
        TokenKind::Case => on_off(value, "case", &mut query.case),
        TokenKind::Hex => hex(query, value),
        TokenKind::Enc => enc(query, value),
        TokenKind::Docs => on_off(value, "docs", &mut query.docs),
        TokenKind::Text
        | TokenKind::Ext
        | TokenKind::Size
        | TokenKind::Modified
        | TokenKind::Attrs
        | TokenKind::Within
        | TokenKind::Archives
        | TokenKind::Git
        | TokenKind::Regex => Ok(()),
    }
}

fn name(query: &mut FindQuery, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(NAME_HELP.to_owned());
    }
    if value.len() > 2 && value.starts_with('/') && value.ends_with('/') {
        let pattern = &value[1..value.len() - 1];
        regex::Regex::new(&format!("(?i){pattern}")).map_err(|_| NAME_HELP.to_owned())?;
    } else {
        for part in value.split(';') {
            let part = part.strip_prefix('!').unwrap_or(part);
            if part.is_empty() || Pattern::parse(part).is_err() {
                return Err(NAME_HELP.to_owned());
            }
        }
    }
    query.name = Some(value.to_owned());
    Ok(())
}

fn depth(query: &mut FindQuery, value: &str) -> Result<(), String> {
    if value.starts_with('+') {
        return Err(DEPTH_HELP.to_owned());
    }
    let depth = value.parse::<u32>().map_err(|_| DEPTH_HELP.to_owned())?;
    if depth > 64 {
        return Err(DEPTH_HELP.to_owned());
    }
    query.depth = Some(depth);
    Ok(())
}

fn hex(query: &mut FindQuery, value: &str) -> Result<(), String> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'-' | b':'))
        .collect();
    if digits.is_empty()
        || !digits.len().is_multiple_of(2)
        || digits.len() > 512
        || !digits.iter().all(u8::is_ascii_hexdigit)
    {
        return Err(HEX_HELP.to_owned());
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair).map_err(|_| HEX_HELP.to_owned())?;
        bytes.push(u8::from_str_radix(pair, 16).map_err(|_| HEX_HELP.to_owned())?);
    }
    query.hex = Some(bytes);
    Ok(())
}

fn enc(query: &mut FindQuery, value: &str) -> Result<(), String> {
    let mut encodings = Vec::new();
    for name in value.split(',') {
        let encoding = match name.trim() {
            "auto" => TextEncoding::Auto,
            "ansi" => TextEncoding::Ansi,
            "dos" => TextEncoding::Dos,
            "utf8" => TextEncoding::Utf8,
            "utf16" => TextEncoding::Utf16,
            _ => return Err(ENC_HELP.to_owned()),
        };
        if encodings.contains(&encoding) {
            return Err(ENC_HELP.to_owned());
        }
        encodings.push(encoding);
    }
    query.encodings = encodings;
    Ok(())
}

fn on_off(value: &str, key: &str, target: &mut bool) -> Result<(), String> {
    *target = match value {
        "on" => true,
        "off" => false,
        _ => return Err(format!("{key}: use on or off")),
    };
    Ok(())
}

/// Maps one query token to the kind its key names.
pub(super) fn token_kind(word: &str) -> TokenKind {
    if word == "-hidden" {
        return TokenKind::Attrs;
    }
    let Some((key, _)) = word.split_once(':') else {
        return TokenKind::Text;
    };
    match key.to_ascii_lowercase().as_str() {
        "ext" => TokenKind::Ext,
        "size" => TokenKind::Size,
        "mod" => TokenKind::Modified,
        "attr" => TokenKind::Attrs,
        "in" => TokenKind::Within,
        "arc" => TokenKind::Archives,
        "git" => TokenKind::Git,
        "re" => TokenKind::Regex,
        "name" => TokenKind::Name,
        "depth" => TokenKind::Depth,
        "word" => TokenKind::Word,
        "not" => TokenKind::Not,
        "case" => TokenKind::Case,
        "hex" => TokenKind::Hex,
        "enc" => TokenKind::Enc,
        "docs" => TokenKind::Docs,
        _ => TokenKind::Text,
    }
}

/// Splits query text into byte spans and whether each token closed its quotes.
pub(super) fn split_tokens(input: &str) -> Vec<(Range<usize>, bool)> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut quoted = false;
    let mut chars = input.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch.is_whitespace() && !quoted {
            if let Some(start) = start.take() {
                tokens.push((start..index, true));
            }
            continue;
        }
        start.get_or_insert(index);
        match ch {
            '"' => quoted = !quoted,
            '\\' if quoted => quoted = split_escape(input, &mut chars),
            _ => {}
        }
    }
    if let Some(start) = start {
        tokens.push((start..input.len(), !quoted));
    }
    tokens
}

/// Says whether a `\` + `"` pair keeps the token quoted.
fn split_escape(input: &str, chars: &mut Peekable<CharIndices<'_>>) -> bool {
    let Some((quote, '"')) = chars.peek().copied() else {
        return true;
    };
    chars.next();
    input[quote + 1..].contains('"')
}

/// Drops the quoting characters from one token and turns `\"` into a literal quote.
pub(super) fn unquote(source: &str) -> String {
    let mut word = String::with_capacity(source.len());
    let mut quoted = false;
    let mut chars = source.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' if quoted => quoted = unquote_escape(source, &mut chars, &mut word),
            _ => word.push(ch),
        }
    }
    word
}

/// Adds a `\` + `"` pair to the word; says whether the token stays quoted.
fn unquote_escape(source: &str, chars: &mut Peekable<CharIndices<'_>>, word: &mut String) -> bool {
    let Some((quote, '"')) = chars.peek().copied() else {
        word.push('\\');
        return true;
    };
    chars.next();
    if source[quote + 1..].contains('"') {
        word.push('"');
        true
    } else {
        word.push('\\');
        false
    }
}

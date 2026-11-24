//! Tokenizes find query text and applies the token kinds that carry a typed value.

use std::iter::Peekable;
use std::ops::Range;
use std::str::CharIndices;

use super::{FindQuery, TokenKind};
use crate::Pattern;

/// Why one token of a find query was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenError {
    UnclosedQuote,
    Size,
    Modified,
    Attribute,
    Within,
    Archives,
    Git,
    Regex,
    RegexSyntax(String),
    Extensions,
    Name,
    Depth { max: u32 },
    Toggle { key: &'static str },
    Hex { max: u32 },
    Encoding,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedQuote => f.write_str("Unclosed quote"),
            Self::Size => f.write_str("size: needs a number, like <50MB"),
            Self::Modified => f.write_str("mod: use 7d, <7d, >2026-10-01 or today"),
            Self::Attribute => {
                f.write_str("attr: use hidden, readonly, system or archive; prefix - to exclude")
            }
            Self::Within => f.write_str("in: needs a folder"),
            Self::Archives => f.write_str("arc: use on or off"),
            Self::Git => f.write_str("git: use on or off"),
            Self::Regex => f.write_str("re: needs a pattern"),
            Self::RegexSyntax(error) => write!(f, "re: {error}"),
            Self::Extensions => f.write_str("ext: needs extensions, like c,h,map"),
            Self::Name => f.write_str("name: use masks like *.rs;!test* or /regex/"),
            Self::Depth { max } => write!(f, "depth: use a number from 0 to {max}"),
            Self::Toggle { key } => write!(f, "{key}: use on or off"),
            Self::Hex { max } => write!(f, "hex: use pairs like 4D 5A, up to {max} bytes"),
            Self::Encoding => f.write_str("enc: use auto, ansi, dos, utf8 or utf16"),
        }
    }
}

/// How find decodes a file while searching it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Auto,
    Ansi,
    Dos,
    Utf8,
    Utf16,
}

const MAX_DEPTH: u32 = 64;
const MAX_HEX_BYTES: u32 = 256;

/// Applies one of the typed token kinds; other kinds are ignored.
pub(super) fn apply(query: &mut FindQuery, kind: TokenKind, value: &str) -> Result<(), TokenError> {
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

fn name(query: &mut FindQuery, value: &str) -> Result<(), TokenError> {
    if value.is_empty() {
        return Err(TokenError::Name);
    }
    if value.len() > 2 && value.starts_with('/') && value.ends_with('/') {
        let pattern = &value[1..value.len() - 1];
        regex::Regex::new(&format!("(?i){pattern}")).map_err(|_| TokenError::Name)?;
    } else {
        for part in value.split(';') {
            let part = part.strip_prefix('!').unwrap_or(part);
            if part.is_empty() || Pattern::parse(part).is_err() {
                return Err(TokenError::Name);
            }
        }
    }
    query.name = Some(value.to_owned());
    Ok(())
}

fn depth(query: &mut FindQuery, value: &str) -> Result<(), TokenError> {
    let error = TokenError::Depth { max: MAX_DEPTH };
    if value.starts_with('+') {
        return Err(error);
    }
    let depth = value.parse::<u32>().map_err(|_| error.clone())?;
    if depth > MAX_DEPTH {
        return Err(error);
    }
    query.depth = Some(depth);
    Ok(())
}

fn hex(query: &mut FindQuery, value: &str) -> Result<(), TokenError> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'-' | b':'))
        .collect();
    if digits.is_empty()
        || !digits.len().is_multiple_of(2)
        || digits.len() > 512
        || !digits.iter().all(u8::is_ascii_hexdigit)
    {
        return Err(TokenError::Hex { max: MAX_HEX_BYTES });
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair).map_err(|_| TokenError::Hex { max: MAX_HEX_BYTES })?;
        bytes.push(
            u8::from_str_radix(pair, 16).map_err(|_| TokenError::Hex { max: MAX_HEX_BYTES })?,
        );
    }
    query.hex = Some(bytes);
    Ok(())
}

fn enc(query: &mut FindQuery, value: &str) -> Result<(), TokenError> {
    let mut encodings = Vec::new();
    for name in value.split(',') {
        let encoding = match name.trim() {
            "auto" => TextEncoding::Auto,
            "ansi" => TextEncoding::Ansi,
            "dos" => TextEncoding::Dos,
            "utf8" => TextEncoding::Utf8,
            "utf16" => TextEncoding::Utf16,
            _ => return Err(TokenError::Encoding),
        };
        if encodings.contains(&encoding) {
            return Err(TokenError::Encoding);
        }
        encodings.push(encoding);
    }
    query.encodings = encodings;
    Ok(())
}

fn on_off(value: &str, key: &'static str, target: &mut bool) -> Result<(), TokenError> {
    *target = match value {
        "on" => true,
        "off" => false,
        _ => return Err(TokenError::Toggle { key }),
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

/// Says whether a `\` + `"` pair keeps the token quoted: a quote followed by
/// whitespace or the end is the closing one, so a Windows folder may end in `\`.
fn split_escape(input: &str, chars: &mut Peekable<CharIndices<'_>>) -> bool {
    let Some((quote, '"')) = chars.peek().copied() else {
        return true;
    };
    chars.next();
    input[quote + 1..]
        .chars()
        .next()
        .is_some_and(|next| !next.is_whitespace())
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
    if source[quote + 1..]
        .chars()
        .next()
        .is_some_and(|next| !next.is_whitespace())
    {
        word.push('"');
        true
    } else {
        word.push('\\');
        false
    }
}

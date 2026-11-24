//! Rules for the find tokens that carry a typed value.

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

/// Whether [`apply`] owns this token kind.
pub(super) fn handles(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Name
            | TokenKind::Depth
            | TokenKind::Word
            | TokenKind::Not
            | TokenKind::Case
            | TokenKind::Hex
            | TokenKind::Enc
            | TokenKind::Docs
    )
}

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
        _ => Ok(()),
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
    if digits.is_empty() || !digits.len().is_multiple_of(2) || digits.len() > 512 {
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

//! Text codecs and invisible-character scan for the F3 editor.
//! Hand-written base64, URL and JSON helpers; no external dependency.

pub use super::invisible::{Invisible, InvisibleKind, scan_invisible};

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const UPPER_HEX: &[u8; 16] = b"0123456789ABCDEF";
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const PADDING: u8 = 0xff;

/// Why decoding failed, with the byte offset into the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// Byte offset into the input where decoding failed, and why (plain English).
    Invalid { at: usize, reason: &'static str },
    /// The decoded bytes are not UTF-8 (the editor holds text only).
    NotUtf8,
}

/// Encodes `text` as standard base64 with `=` padding.
pub fn base64_encode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(BASE64[(b0 >> 2) as usize] as char);
        out.push(BASE64[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Decodes standard base64, skipping ASCII whitespace and accepting missing padding.
pub fn base64_decode(text: &str) -> Result<String, CodecError> {
    let mut symbols = Vec::new();
    let mut offsets = Vec::new();
    for (at, ch) in text.char_indices() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            continue;
        }
        offsets.push(at);
        if ch == '=' {
            symbols.push(PADDING);
        } else if let Some(value) = base64_value(ch) {
            symbols.push(value);
        } else {
            return Err(CodecError::Invalid {
                at,
                reason: "character",
            });
        }
    }
    if symbols.len() % 4 == 1 {
        return Err(CodecError::Invalid {
            at: symbols.len(),
            reason: "length",
        });
    }
    let data_len = symbols
        .iter()
        .position(|&symbol| symbol == PADDING)
        .unwrap_or(symbols.len());
    let padding = symbols.len() - data_len;
    let bad_padding = symbols[data_len..].iter().any(|&symbol| symbol != PADDING)
        || padding > 2
        || (padding == 1 && data_len % 4 != 3)
        || (padding == 2 && data_len % 4 != 2);
    if bad_padding {
        return Err(CodecError::Invalid {
            at: offsets[data_len],
            reason: "padding",
        });
    }
    let bytes = base64_bytes(&symbols[..data_len]);
    String::from_utf8(bytes).map_err(|_| CodecError::NotUtf8)
}

/// Decodes base64 symbols (without padding) into bytes.
fn base64_bytes(symbols: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(symbols.len() / 4 * 3 + 2);
    for chunk in symbols.chunks(4) {
        let c0 = chunk[0];
        let c1 = chunk.get(1).copied().unwrap_or(0);
        let c2 = chunk.get(2).copied().unwrap_or(0);
        let c3 = chunk.get(3).copied().unwrap_or(0);
        bytes.push((c0 << 2) | (c1 >> 4));
        if chunk.len() > 2 {
            bytes.push(((c1 & 0x0f) << 4) | (c2 >> 2));
        }
        if chunk.len() > 3 {
            bytes.push(((c2 & 0x03) << 6) | c3);
        }
    }
    bytes
}

/// Returns the base64 value of `ch`, or `None` when it is not a symbol.
fn base64_value(ch: char) -> Option<u8> {
    match ch {
        'A'..='Z' => Some(ch as u8 - b'A'),
        'a'..='z' => Some(ch as u8 - b'a' + 26),
        '0'..='9' => Some(ch as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

/// Percent-encodes `text`, keeping RFC 3986 unreserved bytes.
pub fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(UPPER_HEX[(byte >> 4) as usize] as char);
            out.push(UPPER_HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

/// Decodes percent escapes, leaving `+` as a literal plus.
pub fn url_decode(text: &str) -> Result<String, CodecError> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        let high = bytes.get(at + 1).copied().and_then(hex_value);
        let low = bytes.get(at + 2).copied().and_then(hex_value);
        let (Some(high), Some(low)) = (high, low) else {
            return Err(CodecError::Invalid {
                at,
                reason: "percent",
            });
        };
        out.push((high << 4) | low);
        at += 3;
    }
    String::from_utf8(out).map_err(|_| CodecError::NotUtf8)
}

/// Escapes `text` for use inside a JSON string, without surrounding quotes.
pub fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let code = ch as u32;
                out.push_str("\\u00");
                out.push(LOWER_HEX[(code >> 4) as usize] as char);
                out.push(LOWER_HEX[(code & 0x0f) as usize] as char);
            }
            ch => out.push(ch),
        }
    }
    out
}

/// Unescapes JSON string content, handling `\/` and surrogate pairs.
pub fn json_unescape(text: &str) -> Result<String, CodecError> {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' {
            let Some(ch) = text[at..].chars().next() else {
                break;
            };
            out.push(ch);
            at += ch.len_utf8();
            continue;
        }
        let (ch, used) = unescape_one(text, at)?;
        out.push(ch);
        at += used;
    }
    Ok(out)
}

/// Decodes one JSON escape at `start`, returning the char and bytes consumed.
fn unescape_one(text: &str, start: usize) -> Result<(char, usize), CodecError> {
    let bytes = text.as_bytes();
    let invalid = || CodecError::Invalid {
        at: start,
        reason: "escape",
    };
    let Some(&kind) = bytes.get(start + 1) else {
        return Err(invalid());
    };
    let simple = match kind {
        b'"' => Some('"'),
        b'\\' => Some('\\'),
        b'/' => Some('/'),
        b'b' => Some('\u{8}'),
        b'f' => Some('\u{c}'),
        b'n' => Some('\n'),
        b'r' => Some('\r'),
        b't' => Some('\t'),
        _ => None,
    };
    if let Some(ch) = simple {
        return Ok((ch, 2));
    }
    if kind != b'u' {
        return Err(invalid());
    }
    let high = hex4(bytes, start + 2).ok_or_else(invalid)?;
    if (0xd800..=0xdbff).contains(&high) {
        let low = hex4(bytes, start + 8).ok_or_else(invalid)?;
        if bytes.get(start + 6) != Some(&b'\\')
            || bytes.get(start + 7) != Some(&b'u')
            || !(0xdc00..=0xdfff).contains(&low)
        {
            return Err(invalid());
        }
        let code = 0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00);
        let ch = char::from_u32(code).ok_or_else(invalid)?;
        return Ok((ch, 12));
    }
    if (0xdc00..=0xdfff).contains(&high) {
        return Err(invalid());
    }
    let ch = char::from_u32(high).ok_or_else(invalid)?;
    Ok((ch, 6))
}

/// Reads four hex digits at `start` as a u32, or `None` when they are not hex.
fn hex4(bytes: &[u8], start: usize) -> Option<u32> {
    let mut value = 0;
    for offset in 0..4 {
        let digit = hex_value(*bytes.get(start + offset)?)?;
        value = (value << 4) | u32::from(digit);
    }
    Some(value)
}

/// Returns the value of one hex digit, or `None` for other bytes.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

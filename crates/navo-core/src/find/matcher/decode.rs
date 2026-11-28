//! Line decoding and UTF-16 BOM detection helpers for the find matcher.

use std::borrow::Cow;

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, WINDOWS_1252};

/// Maximum number of characters kept in a reported hit snippet.
const TEXT_LIMIT: usize = 200;

/// Detects a UTF-16 byte-order mark at the start of the probed file head.
pub(super) fn wide_from_head(head: &[u8]) -> Option<&'static Encoding> {
    match head.get(..2) {
        Some([0xff, 0xfe]) => Some(UTF_16LE),
        Some([0xfe, 0xff]) => Some(UTF_16BE),
        _ => None,
    }
}

/// Number of byte-order-mark bytes to skip for the detected encoding.
pub(super) fn bom_len(wide: Option<&'static Encoding>) -> usize {
    if wide.is_some() { 2 } else { 0 }
}

/// Decodes one raw line, falling back to Windows-1252 for invalid UTF-8.
pub(super) fn decode_line<'a>(bytes: &'a [u8], wide: Option<&'static Encoding>) -> Cow<'a, str> {
    if let Some(encoding) = wide {
        return encoding.decode_without_bom_handling(bytes).0;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => WINDOWS_1252.decode_without_bom_handling(bytes).0,
    }
}

/// Returns at most `TEXT_LIMIT` characters of `text` around `column`.
pub(super) fn snippet(text: &str, column: usize) -> String {
    let length = text.chars().count();
    let start = column
        .saturating_sub(TEXT_LIMIT / 2)
        .min(length.saturating_sub(TEXT_LIMIT));
    text.chars().skip(start).take(TEXT_LIMIT).collect()
}

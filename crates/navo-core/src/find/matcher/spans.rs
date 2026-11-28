//! Byte ranges of the matches inside one line of text, for UI highlighting.
//!
//! Every range lands on a UTF-8 char boundary of the caller's text, so the UI can slice it.

use std::ops::Range;

use aho_corasick::AhoCorasick;
use memchr::memmem;

use super::{Compiled, Kind};

/// Minimum hex digits of the byte offset that prefixes a formatted hex row.
const ROW_PREFIX: usize = 8;
/// Bytes one formatted hex byte takes: two digits and a leading space.
const ROW_BYTE: usize = 3;
/// Bytes one full formatted hex row carries before it wraps to the next row.
const ROW_BYTES: usize = 16;

impl Compiled {
    /// Collects the byte ranges of every match in `text`, replacing `out`.
    ///
    /// Mirrors [`Compiled`]'s scanner: literal and folded text, regex, and the byte pairs of a
    /// formatted hex row. An empty query pushes nothing.
    pub fn spans(&self, text: &str, out: &mut Vec<Range<usize>>) {
        out.clear();
        match &self.kind {
            Kind::Empty => {}
            Kind::Literal(needle) => push_literal(text, needle, out),
            Kind::Fold(matcher) => push_folded(matcher, text, out),
            Kind::Regex(regex) => out.extend(regex.find_iter(text).map(|found| found.range())),
            Kind::Bytes(needle) => push_hex_row(text, needle, out),
        }
    }
}

/// Pushes a range per literal occurrence, matching whole bytes only.
fn push_literal(text: &str, needle: &[u8], out: &mut Vec<Range<usize>>) {
    if needle.is_empty() {
        return;
    }
    let len = needle.len();
    out.extend(memmem::find_iter(text.as_bytes(), needle).map(|start| start..start + len));
}

/// Pushes a range per case-folded occurrence, mapped back to `text`'s char boundaries.
fn push_folded(matcher: &AhoCorasick, text: &str, out: &mut Vec<Range<usize>>) {
    let mut folded = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        for lower in character.to_lowercase() {
            folded.push(lower);
            starts.resize(folded.len(), offset);
        }
    }
    for found in matcher.find_iter(&folded) {
        // The folded byte at `end` belongs to the char after the match, so its start is the end.
        let start = starts[found.start()];
        let mut end = starts.get(found.end()).copied().unwrap_or(text.len());
        if end <= start {
            end = char_end(text, start);
        }
        out.push(start..end);
    }
}

/// The exclusive end offset of the char that starts at `start`, or `start` at the text end.
fn char_end(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .next()
        .map_or(start, |character| start + character.len_utf8())
}

/// Pushes a range per needle occurrence among the byte pairs of a formatted hex row.
///
/// `bytes.rs` writes one row per match, starting on the row where the match begins, so a needle
/// either fits inside that row or continues past the last byte of a full 16-byte row.
fn push_hex_row(text: &str, needle: &[u8], out: &mut Vec<Range<usize>>) {
    let Some(first) = row_bytes_start(text) else {
        return;
    };
    if needle.is_empty() {
        return;
    }
    let pattern = hex_pattern(needle);
    let len = row_len(text, first);
    let mut index = 0;
    while index < len {
        let take = needle.len().min(len - index);
        let whole = take == needle.len();
        let wraps = len == ROW_BYTES && !whole;
        let width = take * ROW_BYTE - 1;
        let text_at = token_span(text, first, index, take);
        if (whole || wraps) && text_at == pattern.get(..width) {
            let start = first + index * ROW_BYTE;
            out.push(start..start + width);
            index += take;
        } else {
            index += 1;
        }
    }
}

/// The text of `take` byte pairs starting at byte pair `index` of the row that starts at `first`.
fn token_span(text: &str, first: usize, index: usize, take: usize) -> Option<&[u8]> {
    let start = first + index * ROW_BYTE;
    text.as_bytes().get(start..start + take * ROW_BYTE - 1)
}

/// The number of byte pairs written in the formatted hex row that starts at `first`.
fn row_len(text: &str, first: usize) -> usize {
    let Some(rest) = text.as_bytes().get(first..) else {
        return 0;
    };
    rest.chunks(ROW_BYTE)
        .take_while(|chunk| chunk.len() >= 2 && chunk[..2].iter().all(u8::is_ascii_hexdigit))
        .count()
}

/// The offset of the first byte pair in a `"XXXXXXXX  4D 5A"` row, when the row has that shape.
///
/// `bytes.rs` writes at least 8 hex digits, so rows past 4 GiB keep their 9 or more digits.
fn row_bytes_start(text: &str) -> Option<usize> {
    let digits = text.bytes().take_while(u8::is_ascii_hexdigit).count();
    if digits < ROW_PREFIX || !text.get(digits..)?.starts_with("  ") {
        return None;
    }
    Some(digits + 2)
}

/// The upper-case `"4D 5A"` form of `needle`, as a formatted hex row writes it.
fn hex_pattern(needle: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut pattern = Vec::with_capacity(needle.len() * ROW_BYTE);
    for byte in needle {
        if !pattern.is_empty() {
            pattern.push(b' ');
        }
        pattern.push(DIGITS[usize::from(byte >> 4)]);
        pattern.push(DIGITS[usize::from(byte & 0x0F)]);
    }
    pattern
}

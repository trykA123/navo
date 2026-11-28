//! Byte ranges of the matches inside one line of text, for UI highlighting.
//!
//! Every range lands on a UTF-8 char boundary of the caller's text, so the UI can slice it.

use std::ops::Range;

use aho_corasick::AhoCorasick;
use memchr::memmem;

use super::{Compiled, Kind};

/// Hex digits of the byte offset that prefixes a formatted hex row.
const ROW_PREFIX: usize = 8;
/// Characters one formatted hex byte takes: two digits and a leading space.
const ROW_BYTE: usize = 3;

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
fn push_hex_row(text: &str, needle: &[u8], out: &mut Vec<Range<usize>>) {
    let Some(first) = row_bytes_start(text) else {
        return;
    };
    if needle.is_empty() {
        return;
    }
    let pattern = hex_pattern(needle);
    let len = pattern.len();
    for start in memmem::find_iter(text.as_bytes(), &pattern) {
        let aligned = start >= first && (start - first).is_multiple_of(ROW_BYTE);
        if aligned && start + len <= text.len() {
            out.push(start..start + len);
        }
    }
}

/// The offset of the first byte pair in a `"XXXXXXXX  4D 5A"` row, when the row has that shape.
fn row_bytes_start(text: &str) -> Option<usize> {
    let digits = text.get(..ROW_PREFIX)?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rest = text.get(ROW_PREFIX..)?;
    let skipped = rest.bytes().take_while(|byte| *byte == b' ').count();
    (skipped > 0).then(|| ROW_PREFIX + skipped)
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

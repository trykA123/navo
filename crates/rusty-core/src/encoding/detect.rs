use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, REPLACEMENT, UTF_8};

use super::{DetectedBy, TextEncoding, utf16};

const DECLARATION_WINDOW: usize = 1024;
const GUESS_WINDOW: usize = 64 * 1024;
const MIN_NON_ASCII: usize = 4;
const VALID_PER_INVALID: usize = 4;
const HTML_STARTS: [&[u8]; 3] = [b"<!doctype html", b"<html", b"<head"];

pub(super) fn bom(bytes: &[u8]) -> Option<(&'static Encoding, usize)> {
    if utf16::is_utf32_le(bytes) {
        return None;
    }
    Encoding::for_bom(bytes)
}

pub(super) fn found(encoding: &'static Encoding, bom: bool, by: DetectedBy) -> TextEncoding {
    TextEncoding { encoding, bom, by }
}

pub(super) fn detect(bytes: &[u8], fallback: &'static Encoding) -> TextEncoding {
    if let Some((encoding, _)) = bom(bytes) {
        return found(encoding, true, DetectedBy::Bom);
    }
    if let Some(encoding) = utf16::by_pattern(bytes) {
        return found(encoding, false, DetectedBy::Guess);
    }
    if let Some(encoding) = declared(bytes) {
        return found(encoding, false, DetectedBy::Declaration);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return found(UTF_8, false, DetectedBy::Utf8);
    }
    if mostly_utf8(bytes) {
        return found(UTF_8, false, DetectedBy::Guess);
    }
    let window = &bytes[..bytes.len().min(GUESS_WINDOW)];
    if window.iter().filter(|byte| !byte.is_ascii()).count() >= MIN_NON_ASCII {
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        detector.feed(window, true);
        return found(
            detector.guess(None, Utf8Detection::Deny),
            false,
            DetectedBy::Guess,
        );
    }
    found(fallback, false, DetectedBy::Fallback)
}

fn declared(bytes: &[u8]) -> Option<&'static Encoding> {
    let head = &bytes[..bytes.len().min(DECLARATION_WINDOW)];
    let lower: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();
    let label = if lower.starts_with(b"<?xml") {
        let end = find(&lower, b"?>").unwrap_or(lower.len());
        attribute(&head[..end], &lower[..end], b"encoding")
    } else if is_html(&lower) {
        find(&lower, b"<meta").and_then(|at| attribute(&head[at..], &lower[at..], b"charset"))
    } else {
        None
    }?;
    Encoding::for_label(label)
        .filter(|encoding| *encoding != REPLACEMENT && !utf16::is_utf16(encoding))
}

fn is_html(lower: &[u8]) -> bool {
    let start = lower.trim_ascii_start();
    HTML_STARTS.iter().any(|tag| start.starts_with(tag))
}

fn mostly_utf8(bytes: &[u8]) -> bool {
    let (mut rest, mut invalid, mut multi) = (bytes, 0_usize, 0_usize);
    while let Err(error) = std::str::from_utf8(rest) {
        let (valid, after) = rest.split_at(error.valid_up_to());
        multi += lead_bytes(valid);
        invalid += 1;
        rest = error.error_len().map_or(&[], |len| &after[len..]);
    }
    multi += lead_bytes(rest);
    multi > 0 && invalid * VALID_PER_INVALID <= multi
}

fn lead_bytes(valid: &[u8]) -> usize {
    valid.iter().filter(|byte| **byte >= 0xC0).count()
}

fn attribute<'a>(head: &'a [u8], lower: &[u8], name: &[u8]) -> Option<&'a [u8]> {
    let at = find(lower, name)? + name.len();
    let rest = head.get(at..)?;
    let rest = rest
        .trim_ascii_start()
        .strip_prefix(b"=")?
        .trim_ascii_start();
    let (quote, rest) = match rest.first() {
        Some(q @ (b'"' | b'\'')) => (Some(*q), &rest[1..]),
        _ => (None, rest),
    };
    let end = rest
        .iter()
        .position(|byte| match quote {
            Some(q) => *byte == q,
            None => byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b'>' | b'/' | b';'),
        })
        .unwrap_or(rest.len());
    Some(&rest[..end]).filter(|label| !label.is_empty())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len())
        .position(|window| window == needle)
}

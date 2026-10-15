use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE};

use super::{DetectedBy, TextEncoding};

const DECLARATION_WINDOW: usize = 1024;
const MIN_NON_ASCII: usize = 4;
const UTF16_SAMPLE: usize = 4096;

pub(super) fn bom(bytes: &[u8]) -> Option<(&'static Encoding, usize)> {
    Encoding::for_bom(bytes)
}

pub(super) fn found(encoding: &'static Encoding, bom: bool, by: DetectedBy) -> TextEncoding {
    TextEncoding { encoding, bom, by }
}

pub(super) fn detect(bytes: &[u8], fallback: &'static Encoding) -> TextEncoding {
    if let Some((encoding, _)) = bom(bytes) {
        return found(encoding, true, DetectedBy::Bom);
    }
    if let Some(encoding) = utf16_by_pattern(bytes) {
        return found(encoding, false, DetectedBy::Guess);
    }
    if let Some(encoding) = declared(bytes) {
        return found(encoding, false, DetectedBy::Declaration);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return found(encoding_rs::UTF_8, false, DetectedBy::Utf8);
    }
    if bytes.iter().filter(|byte| !byte.is_ascii()).count() >= MIN_NON_ASCII {
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        detector.feed(bytes, true);
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
    } else {
        find(&lower, b"<meta").and_then(|at| attribute(&head[at..], &lower[at..], b"charset"))
    }?;
    Encoding::for_label(label).filter(|encoding| *encoding != UTF_16LE && *encoding != UTF_16BE)
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

pub(super) fn utf16_by_pattern(bytes: &[u8]) -> Option<&'static Encoding> {
    let sample = &bytes[..bytes.len().min(UTF16_SAMPLE) & !1];
    let pairs = sample.len() / 2;
    if pairs < 4 {
        return None;
    }
    let zeros = |offset: usize| {
        sample
            .iter()
            .skip(offset)
            .step_by(2)
            .filter(|b| **b == 0)
            .count()
    };
    let (even, odd) = (zeros(0), zeros(1));
    let mostly = |count: usize| count * 10 >= pairs * 7;
    let rarely = |count: usize| count * 20 <= pairs;
    if mostly(odd) && rarely(even) {
        Some(UTF_16LE)
    } else if mostly(even) && rarely(odd) {
        Some(UTF_16BE)
    } else {
        None
    }
}

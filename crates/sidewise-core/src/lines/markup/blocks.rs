//! XML block delimiters and content for the streaming lexer.
use super::*;

const CDATA_OPEN: &[u8] = b"<![CDATA[";
const COMMENT_OPEN: &[u8] = b"<!--";

/// Continues a possible opening delimiter, retaining incomplete bytes across chunks.
pub(super) fn opening(
    bytes: &[u8],
    cursor: (&mut usize, &mut u32),
    delimiter: (&mut usize, &mut u32),
    spans: &mut Vec<Span>,
) {
    let (i, mode) = cursor;
    let (prefix, partial) = delimiter;
    let start = *i;
    while *i < bytes.len() {
        let b = bytes[*i];
        if *prefix == 1 && (name_start(b) || b == b'/') {
            if b == b'/' {
                push(spans, *i, *i + 1, "punctuation.delimiter");
                *i += 1;
            }
            *mode = TAG_NAME;
            *prefix = 0;
            return;
        }
        if *prefix == 1 && b == b'?' || *prefix == 2 && b.is_ascii_alphabetic() {
            *i += 1;
            *mode = DECL | if b == b'?' { PROCESSING } else { 0 };
            *prefix = 0;
            push(spans, start, *i, "keyword");
            return;
        }
        let comment = *prefix < COMMENT_OPEN.len() && COMMENT_OPEN[*prefix] == b;
        let cdata = *prefix < CDATA_OPEN.len() && CDATA_OPEN[*prefix] == b;
        if !comment && !cdata {
            *prefix = 0;
            return;
        }
        *i += 1;
        *prefix += 1;
        if *prefix == COMMENT_OPEN.len() && comment {
            *prefix = 0;
            *mode = COMMENT;
            comment_body(bytes, (i, mode), (partial, start), spans);
            return;
        }
        if *prefix == CDATA_OPEN.len() {
            *prefix = 0;
            *mode = CDATA;
            push(spans, start, *i, "punctuation.delimiter");
            return;
        }
    }
}

/// Scans a comment body after its opener and extends the first span back to `start`.
fn comment_body(
    bytes: &[u8],
    cursor: (&mut usize, &mut u32),
    carry: (&mut u32, usize),
    spans: &mut Vec<Span>,
) {
    let (partial, start) = carry;
    let before = spans.len();
    let i = cursor.0;
    block(bytes, (i, cursor.1), partial, spans);
    if let Some((range, _)) = spans.get_mut(before) {
        range.start = start;
    } else {
        push(spans, start, *i, "comment");
    }
}

/// Scans block content and carries incomplete closing delimiters.
pub(super) fn block(
    bytes: &[u8],
    cursor: (&mut usize, &mut u32),
    partial: &mut u32,
    spans: &mut Vec<Span>,
) {
    let (i, mode) = cursor;
    let start = *i;
    let original = *mode & 0xff;
    let processing = *mode & PROCESSING != 0;
    while *i < bytes.len() {
        let b = bytes[*i];
        *i += 1;
        let closes = match original {
            COMMENT => b == b'>' && *partial == 2,
            CDATA => b == b'>' && *partial == 2,
            DECL => b == b'>' && (!processing || *partial == 1),
            _ => false,
        };
        if closes {
            if original == CDATA {
                let begin = (*i - 1).saturating_sub(2).max(start);
                push(spans, begin, *i, "punctuation.delimiter");
            }
            *mode = TEXT;
            *partial = 0;
            break;
        }
        *partial = match original {
            COMMENT if b == b'-' => (*partial + 1).min(2),
            CDATA if b == b']' => (*partial + 1).min(2),
            DECL if b == b'?' => 1,
            _ => 0,
        };
    }
    if original == COMMENT {
        push(spans, start, *i, "comment");
    }
    if original == DECL {
        push(spans, start, *i, "keyword");
    }
}

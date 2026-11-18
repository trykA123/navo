//! XML text, tags, attributes and values for the streaming lexer.
use super::*;

/// Scans plain text for tag starts and complete entities.
pub(super) fn text(
    bytes: &[u8],
    cursor: (&mut usize, &mut u32),
    delimiter: (&mut usize, &mut u32),
    spans: &mut Vec<Span>,
) {
    let (i, mode) = cursor;
    let (prefix, partial) = delimiter;
    let end = bytes.len();
    if bytes[*i] == b'<' {
        if *i + 1 == end {
            *prefix = 1;
            *i += 1;
            return;
        }
        if matches!(bytes[*i + 1], b'!' | b'?') {
            *prefix = 1;
            *i += 1;
            let before = spans.len();
            opening(bytes, (i, mode), (prefix, partial), spans);
            if let Some((range, _)) = spans.get_mut(before) {
                range.start -= 1;
            }
            return;
        }
        if bytes[*i + 1] == b'/' || name_start(bytes[*i + 1]) {
            let len = if bytes[*i + 1] == b'/' { 2 } else { 1 };
            push(spans, *i, *i + len, "punctuation.delimiter");
            *i += len;
            *mode = TAG_NAME;
            return;
        }
    } else if bytes[*i] == b'&' {
        entity(bytes, (i, mode), spans);
        return;
    }
    *i += 1;
}

/// Scans an `&...;` entity, colouring it when complete and carrying its mode at chunk end.
fn entity(bytes: &[u8], cursor: (&mut usize, &mut u32), spans: &mut Vec<Span>) {
    let (i, mode) = cursor;
    let end = bytes.len();
    let start = *i;
    *i += 1;
    while *i < end && entity_char(bytes[*i]) {
        *i += 1;
    }
    let entity = &bytes[start + 1..*i];
    let valid = entity.first().is_some_and(|b| name_start(*b))
        && entity.iter().copied().all(name_char)
        || entity.first() == Some(&b'#')
            && entity.len() > 1
            && entity[1..].iter().all(u8::is_ascii_digit);
    if *i < end && bytes[*i] == b';' && valid {
        *i += 1;
        push(spans, start, *i, "constant");
    } else if *i == end {
        *mode = ENTITY;
    }
}

/// Scans a tag name, carrying its mode when the chunk ends.
pub(super) fn name(bytes: &[u8], i: &mut usize, mode: &mut u32, spans: &mut Vec<Span>) {
    let end = bytes.len();
    let start = *i;
    while *i < end && name_char(bytes[*i]) {
        *i += 1;
    }
    push(spans, start, *i, "tag");
    if *i < end {
        *mode = TAG;
    }
}

/// Scans attributes, values and the closing tag delimiter.
pub(super) fn tag(bytes: &[u8], i: &mut usize, mode: &mut u32, spans: &mut Vec<Span>) {
    let end = bytes.len();
    let start = *i;
    match bytes[*i] {
        b'>' => {
            *i += 1;
            *mode = TEXT;
            push(spans, start, *i, "punctuation.delimiter");
        }
        b'/' if *i + 1 < end && bytes[*i + 1] == b'>' => {
            *i += 2;
            *mode = TEXT;
            push(spans, start, *i, "punctuation.delimiter");
        }
        b'=' => {
            *i += 1;
            push(spans, start, *i, "punctuation.delimiter");
        }
        b'"' | b'\'' => {
            *mode = if bytes[*i] == b'"' { DOUBLE } else { SINGLE };
            *i += 1;
            value(bytes, i, mode, spans);
            if let Some((range, "string")) = spans.last_mut()
                && range.start == start + 1
            {
                range.start = start;
                return;
            }
            push(spans, start, *i, "string");
        }
        b if name_start(b) => {
            *i += 1;
            while *i < end && name_char(bytes[*i]) {
                *i += 1;
            }
            push(spans, start, *i, "attribute");
        }
        _ => *i += 1,
    }
}

/// Scans a quoted value through its closing quote or chunk end.
pub(super) fn value(bytes: &[u8], i: &mut usize, mode: &mut u32, spans: &mut Vec<Span>) {
    let end = bytes.len();
    let start = *i;
    let quote = if *mode == DOUBLE { b'"' } else { b'\'' };
    while *i < end {
        let current = bytes[*i];
        *i += 1;
        if current == quote {
            *mode = TAG;
            break;
        }
    }
    push(spans, start, *i, "string");
}

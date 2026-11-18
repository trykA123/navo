//! Streaming XML tag and value colouring for line lexers.
use super::{LineState, MAX_SPANS, Span};

mod blocks;
mod tags;

use blocks::{block, opening};
use tags::{name, tag, text, value};

const TEXT: u32 = 0;
const TAG_NAME: u32 = 1;
const TAG: u32 = 2;
const DOUBLE: u32 = 3;
const SINGLE: u32 = 4;
const ENTITY: u32 = 5;
const COMMENT: u32 = 6;
const CDATA: u32 = 7;
const DECL: u32 = 8;
const PROCESSING: u32 = 1 << 17;

/// Colours one XML chunk and returns its carried state.
pub(super) fn lex(chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
    let bytes = chunk.as_bytes();
    let end = bytes.len();
    let mut spans = Vec::with_capacity((chunk.len() / 2).min(MAX_SPANS));
    let mut mode = match state.0 & 0xff {
        0..=8 => (state.0 & 0xff) | (state.0 & PROCESSING),
        _ => TEXT,
    };
    let mut index = 0;
    let mut prefix = if mode == TEXT { state.prefix_len() } else { 0 };
    let mut partial = (state.0 >> 12) & 0xf;
    while index < end {
        if prefix > 0 {
            opening(
                bytes,
                (&mut index, &mut mode),
                (&mut prefix, &mut partial),
                &mut spans,
            );
            continue;
        }
        match mode & 0xff {
            TEXT => text(
                bytes,
                (&mut index, &mut mode),
                (&mut prefix, &mut partial),
                &mut spans,
            ),
            TAG_NAME => name(bytes, &mut index, &mut mode, &mut spans),
            TAG => tag(bytes, &mut index, &mut mode, &mut spans),
            DOUBLE | SINGLE => value(bytes, &mut index, &mut mode, &mut spans),
            COMMENT | CDATA | DECL => {
                block(bytes, (&mut index, &mut mode), &mut partial, &mut spans)
            }
            ENTITY => entity_step(bytes[index], &mut index, &mut mode),
            _ => unreachable!(),
        }
    }
    (
        spans,
        LineState(mode | ((prefix as u32) << 8) | (partial << 12)),
    )
}

/// Advances one byte of an entity carried across chunks, leaving entity mode when it ends.
fn entity_step(b: u8, index: &mut usize, mode: &mut u32) {
    if entity_char(b) {
        *index += 1;
    } else if b == b';' {
        *index += 1;
        *mode = TEXT;
    } else {
        *mode = TEXT;
    }
}

/// Clears delimiter partials and ends tag names and entities at line breaks.
pub(super) fn line_end(state: LineState) -> LineState {
    let cleared = state.0 & !0x1ff00;
    LineState(match cleared & 0xff {
        TAG_NAME => (cleared & !0xff) | TAG,
        ENTITY => cleared & !0xff,
        _ => cleared,
    })
}

/// Emits one bounded span, leaving later scanning active after the cap.
fn push(spans: &mut Vec<Span>, start: usize, end: usize, role: &'static str) {
    if start < end && spans.len() < MAX_SPANS {
        spans.push((start..end, role));
    }
}

/// Recognises the first byte of an XML name.
fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
}

/// Recognises a byte after the first byte of an XML name.
fn name_char(byte: u8) -> bool {
    name_start(byte) || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
}

/// Recognises a possible entity byte after the ampersand.
fn entity_char(byte: u8) -> bool {
    name_char(byte) || byte == b'#'
}

/// Infers an XML state from the first decisive page delimiter.
pub(super) fn resync(head: &str) -> Option<LineState> {
    let bytes = head.as_bytes();
    for i in 0..bytes.len() {
        let rest = &bytes[i..];
        if rest.starts_with(b"-->") {
            return Some(LineState(COMMENT));
        }
        if rest.starts_with(b"]]>") {
            return Some(LineState(CDATA));
        }
        if rest.first() == Some(&b'<')
            && rest
                .get(1)
                .is_some_and(|b| name_start(*b) || matches!(b, b'/' | b'!' | b'?'))
        {
            return Some(LineState(TEXT));
        }
    }
    None
}

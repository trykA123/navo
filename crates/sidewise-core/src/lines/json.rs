//! Streaming JSON token and string colouring for line lexers.
use super::{LineState, MAX_SPANS, Span};

const STRING: u32 = 1;
const BLOCK_COMMENT: u32 = 2;
const LINE_COMMENT: u32 = 3;
const PREFIX: u32 = 1 << 8;
const CLOSING_STAR: u32 = 1 << 12;
const ESCAPE: u32 = 1 << 16;
const NUMBER_SHIFT: u32 = 20;

/// Parse stage of a number cut off by the chunk end (carried in bits 20..23).
const NUM_NONE: u32 = 0;
const NUM_INT: u32 = 1;
const NUM_DOT: u32 = 2;
const NUM_FRAC: u32 = 3;
const NUM_E: u32 = 4;
const NUM_SIGN: u32 = 5;
const NUM_EXP: u32 = 6;

/// Holds the state while scanning one JSON chunk.
struct Scanner<'a> {
    bytes: &'a [u8],
    index: usize,
    mode: u32,
    escape: bool,
    star: bool,
    prefix: bool,
    number: u32,
    tail: usize,
}

/// Colours a JSON chunk and returns its carried lexer state.
pub(super) fn lex(chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
    let mode = state.0 & 0xff;
    let mut scan = Scanner {
        bytes: chunk.as_bytes(),
        index: 0,
        mode,
        escape: mode == STRING && state.0 & ESCAPE != 0,
        star: mode == BLOCK_COMMENT && state.0 & CLOSING_STAR != 0,
        prefix: state.0 & PREFIX != 0,
        number: if mode == 0 {
            (state.0 >> NUMBER_SHIFT) & 7
        } else {
            NUM_NONE
        },
        tail: 0,
    };
    let mut spans = Vec::with_capacity((chunk.len() / 2).min(MAX_SPANS));
    while scan.index < scan.bytes.len() {
        let start = scan.index;
        if let Some(role) = scan.next() {
            push(&mut spans, start..scan.index - scan.tail, role);
        }
    }
    let flags = if scan.escape { ESCAPE } else { 0 }
        | if scan.star { CLOSING_STAR } else { 0 }
        | if scan.prefix { PREFIX } else { 0 }
        | scan.number << NUMBER_SHIFT;
    (spans, LineState(scan.mode | flags))
}

impl Scanner<'_> {
    /// Scans the next token and returns its role, if coloured.
    fn next(&mut self) -> Option<&'static str> {
        self.tail = 0;
        if self.prefix {
            self.prefix = false;
            if matches!(self.bytes[self.index], b'/' | b'*') {
                self.start_comment(1);
                return Some("comment");
            }
        }
        if self.mode == 0 && self.number_token() {
            return Some("number");
        }
        match self.mode {
            STRING => Some(self.string()),
            BLOCK_COMMENT => {
                scan_comment(self.bytes, &mut self.index, &mut self.mode, &mut self.star);
                Some("comment")
            }
            LINE_COMMENT => {
                self.index = self.bytes.len();
                Some("comment")
            }
            _ if self.bytes[self.index] == b'"' => {
                self.mode = STRING;
                self.index += 1;
                Some(self.string())
            }
            _ if self.bytes[self.index] == b'/' && self.index + 1 == self.bytes.len() => {
                self.prefix = true;
                self.index += 1;
                None
            }
            _ if self.bytes[self.index] == b'/'
                && matches!(self.bytes[self.index + 1], b'/' | b'*') =>
            {
                self.start_comment(2);
                Some("comment")
            }
            _ => {
                let result = token(self.bytes, self.index);
                self.index = result.map_or(self.index + 1, |(end, _)| end);
                result.map(|(_, role)| role)
            }
        }
    }

    /// Scans a number or the continuation of one cut by the last chunk end.
    ///
    /// Returns false when the byte starts no number. An open tail is consumed
    /// uncoloured (`tail`) and its stage is carried to the next chunk.
    fn number_token(&mut self) -> bool {
        let stage = std::mem::replace(&mut self.number, NUM_NONE);
        let mut found = (self.index, NUM_NONE);
        if stage != NUM_NONE {
            found = number(self.bytes, self.index, stage);
        }
        // A failed continuation retries as a fresh number at the same byte.
        if found == (self.index, NUM_NONE) && matches!(self.bytes[self.index], b'-' | b'0'..=b'9') {
            found = number(self.bytes, self.index, NUM_NONE);
        }
        let (committed, open) = found;
        if committed == self.index && open == NUM_NONE {
            return false;
        }
        let consumed = if open == NUM_NONE {
            committed
        } else {
            self.bytes.len()
        };
        self.tail = consumed - committed;
        self.index = consumed;
        self.number = open;
        true
    }

    /// Scans a string and chooses its role using this chunk's following bytes.
    fn string(&mut self) -> &'static str {
        scan_string(
            self.bytes,
            &mut self.index,
            &mut self.mode,
            &mut self.escape,
        );
        string_role(self.bytes, self.index, self.mode)
    }

    /// Starts a line or block comment after consuming its opening bytes.
    fn start_comment(&mut self, opening: usize) {
        self.mode = if self.bytes[self.index + opening - 1] == b'/' {
            LINE_COMMENT
        } else {
            BLOCK_COMMENT
        };
        self.index += opening;
        self.star = false;
        if self.mode == LINE_COMMENT {
            self.index = self.bytes.len();
        } else {
            scan_comment(self.bytes, &mut self.index, &mut self.mode, &mut self.star);
        }
    }
}

/// Adds a non-empty span while respecting the per-chunk cap.
fn push(spans: &mut Vec<Span>, range: std::ops::Range<usize>, role: &'static str) {
    if spans.len() < MAX_SPANS && !range.is_empty() {
        spans.push((range, role));
    }
}

/// Scans string bytes, updating the offset and carried string state.
fn scan_string(bytes: &[u8], index: &mut usize, mode: &mut u32, escape: &mut bool) {
    while *index < bytes.len() {
        let byte = bytes[*index];
        *index += 1;
        if *escape {
            *escape = false;
        } else if byte == b'\\' {
            *escape = true;
        } else if byte == b'"' {
            *mode = 0;
            break;
        }
    }
}

/// Selects property only when the closed string has a colon in this chunk.
fn string_role(bytes: &[u8], index: usize, mode: u32) -> &'static str {
    if mode == 0
        && bytes[index..]
            .iter()
            .copied()
            .find(|b| !b.is_ascii_whitespace())
            == Some(b':')
    {
        "property"
    } else {
        "string"
    }
}

/// Scans a block comment, carrying a closing star across chunks.
fn scan_comment(bytes: &[u8], index: &mut usize, mode: &mut u32, star: &mut bool) {
    while *index < bytes.len() {
        let byte = bytes[*index];
        *index += 1;
        if *star && byte == b'/' {
            *mode = 0;
            *star = false;
            break;
        }
        *star = byte == b'*';
    }
}

/// Clears delimiter and escape state and ends line comments at a line break.
pub(super) fn line_end(state: LineState) -> LineState {
    let mode = state.0 & 0xff;
    LineState(if mode == LINE_COMMENT { 0 } else { mode })
}

/// Guesses whether a page begins inside a block comment.
pub(super) fn resync(head: &str) -> Option<LineState> {
    let close = head.find("*/");
    let open = head.find("/*");
    Some(LineState(
        if close.is_some() && (open.is_none() || close < open) {
            BLOCK_COMMENT
        } else {
            0
        },
    ))
}

/// Finds a token beginning at the byte offset, or returns none.
fn token(bytes: &[u8], start: usize) -> Option<(usize, &'static str)> {
    let rest = &bytes[start..];
    let role = match rest[0] {
        b'{' | b'}' | b'[' | b']' => "punctuation.bracket",
        b':' | b',' => "punctuation.delimiter",
        _ => {
            for (word, role) in [
                (b"true".as_slice(), "boolean"),
                (b"false", "boolean"),
                (b"null", "constant"),
            ] {
                if rest.starts_with(word) {
                    return Some((start + word.len(), role));
                }
            }
            return None;
        }
    };
    Some((start + 1, role))
}

/// Scans a JSON number, or the rest of one cut by a chunk end, from `start`.
///
/// Returns the end of the valid number prefix and, when the scan ran into the chunk
/// end with a number still open, the stage to resume from (otherwise `NUM_NONE`).
fn number(bytes: &[u8], start: usize, stage: u32) -> (usize, u32) {
    let mut index = start;
    let mut stage = stage;
    if stage == NUM_NONE {
        index += usize::from(bytes[start] == b'-');
        if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return (start, NUM_NONE);
        }
        stage = NUM_INT;
        index += 1;
    }
    let mut committed = index;
    while let Some(&byte) = bytes.get(index) {
        let digit = byte.is_ascii_digit();
        let (next, commits) = match (stage, byte) {
            (NUM_INT | NUM_FRAC | NUM_EXP, _) if digit => (stage, true),
            (NUM_INT, b'.') => (NUM_DOT, false),
            (NUM_DOT, _) if digit => (NUM_FRAC, true),
            (NUM_INT | NUM_FRAC, b'e' | b'E') => (NUM_E, false),
            (NUM_E, b'+' | b'-') => (NUM_SIGN, false),
            (NUM_E | NUM_SIGN, _) if digit => (NUM_EXP, true),
            _ => return (committed, NUM_NONE),
        };
        index += 1;
        stage = next;
        if commits {
            committed = index;
        }
    }
    (committed, stage)
}

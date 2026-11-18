//! Streaming Log/plain-text lexer: strings, level words, numbers and shapes; dispatched from mod.rs.
//!
//! A token that reaches the end of a chunk that may continue is unresolved: it gets no span and
//! the lexer carries a skip mode to the next whitespace (D7: under-colouring is allowed).
use super::log_shapes::shape;
use super::log_words::{
    CLASS, DOUBLE, OPENER, PLAIN, SINGLE, is_word, level, number_end, run_end, word_end,
};
use super::{LineState, MAX_SPANS, Span};

const MODE_MASK: u32 = 0xff;
const NORMAL: u32 = 0;
const IN_DOUBLE: u32 = 1;
/// Skips an unresolved token up to the next whitespace or an allowed quote.
const SKIP: u32 = 2;
/// Carries a single-quoted candidate whose close is not seen yet.
const IN_SINGLE: u32 = 3;
const ESCAPE: u32 = 1 << 16;
/// The last byte before the chunk is a word byte (modes SKIP and IN_SINGLE).
const PREV_WORD: u32 = 1 << 17;
/// The candidate's last byte was a `'` whose next byte is unknown (mode IN_SINGLE).
const PENDING: u32 = 1 << 18;
/// Bits 24..32 hold the bytes the single-quoted candidate has used so far.
const COUNT_SHIFT: u32 = 24;

/// Longest single-quoted string, quotes included, that one lexer may carry.
const SINGLE_MAX: usize = 200;

/// What follows a `'` inside a single-quoted candidate.
enum Follow {
    /// A non-word byte or the line end: the quote closes the string.
    Boundary,
    /// A word byte: the quote is part of the string.
    Word,
    /// The chunk ends on the quote and may continue: the next chunk decides.
    Unknown,
}

/// Holds the carried mode while scanning one Log chunk.
struct Scanner<'a> {
    chunk: &'a str,
    bytes: &'a [u8],
    /// True when the chunk ends its line, so a token at the chunk end is complete.
    last: bool,
    /// Tokens starting at or after this offset are unresolved: no whitespace follows them.
    limit: usize,
    index: usize,
    mode: u32,
    flags: u32,
    count: usize,
    spans: Vec<Span>,
}

/// Returns the state flags a mode carries across a chunk end.
fn kept_flags(mode: u32) -> u32 {
    match mode {
        IN_DOUBLE => ESCAPE,
        SKIP => PREV_WORD,
        IN_SINGLE => PREV_WORD | PENDING,
        _ => 0,
    }
}

/// Colours a Log chunk and returns its carried lexer state; `last` ends the line.
pub(super) fn lex(chunk: &str, state: LineState, last: bool) -> (Vec<Span>, LineState) {
    let bytes = chunk.as_bytes();
    let limit = if last {
        bytes.len()
    } else {
        bytes
            .iter()
            .rposition(|b| b.is_ascii_whitespace())
            .map_or(0, |at| at + 1)
    };
    let mode = state.0 & MODE_MASK;
    let mut scan = Scanner {
        chunk,
        bytes,
        last,
        limit,
        index: 0,
        mode,
        flags: state.0 & kept_flags(mode),
        count: ((state.0 >> COUNT_SHIFT) as usize).min(SINGLE_MAX),
        spans: Vec::with_capacity((chunk.len() / 2).min(MAX_SPANS)),
    };
    while scan.index < bytes.len() {
        match scan.mode {
            IN_DOUBLE => scan.double_tail(scan.index),
            SKIP => scan.skip(),
            IN_SINGLE => scan.single_scan(scan.index, true),
            _ => scan.step(),
        }
    }
    scan.finish()
}

/// Clears every carried Log mode and escape at a line break.
pub(super) fn line_end(_state: LineState) -> LineState {
    LineState(0)
}

impl Scanner<'_> {
    /// Packs the carried mode, flags and single-quote count into a state.
    fn finish(mut self) -> (Vec<Span>, LineState) {
        if let (SKIP | IN_SINGLE, Some(&byte)) = (self.mode, self.bytes.last()) {
            self.flags = (self.flags & PENDING) | if is_word(byte) { PREV_WORD } else { 0 };
        }
        let count = if self.mode == IN_SINGLE {
            self.count
        } else {
            0
        };
        let flags = self.flags & kept_flags(self.mode);
        (
            self.spans,
            LineState(self.mode | flags | ((count as u32) << COUNT_SHIFT)),
        )
    }

    /// Records a span unless the chunk cap is reached; the scan goes on either way.
    fn emit(&mut self, start: usize, end: usize, role: &'static str) {
        if start < end && self.spans.len() < MAX_SPANS {
            self.spans.push((start..end, role));
        }
    }

    /// Returns true when the byte before `at` is a word byte, so no token may start there.
    fn prev_word(&self, at: usize) -> bool {
        match at.checked_sub(1) {
            Some(before) => is_word(self.bytes[before]),
            None => self.flags & PREV_WORD != 0,
        }
    }

    /// Scans one plain run or one token in the default mode.
    fn step(&mut self) {
        let at = self.index;
        let class = CLASS[self.bytes[at] as usize];
        let quote = class == DOUBLE || class == SINGLE;
        if at >= self.limit && !quote {
            self.mode = SKIP;
        } else if class == PLAIN {
            let mut end = at + 1;
            while end < self.limit && CLASS[self.bytes[end] as usize] == PLAIN {
                end += 1;
            }
            self.index = end;
        } else if self.prev_word(at) {
            self.index += 1;
        } else if class == DOUBLE {
            self.mode = IN_DOUBLE;
            self.flags = 0;
            self.index += 1;
            self.double_tail(at);
        } else if class == SINGLE {
            self.count = 1;
            self.index += 1;
            self.single_scan(at, false);
        } else {
            self.token();
        }
    }

    /// Colours the shape, number or word that starts at the current byte.
    fn token(&mut self) {
        let at = self.index;
        let first = self.bytes[at];
        if CLASS[first as usize] == OPENER {
            self.opener(at, first);
            return;
        }
        let run = run_end(self.bytes, at);
        let (digit, size) = (first.is_ascii_digit(), run - at);
        let shaped = match self.bytes.get(run) {
            Some(b':' | b'-') => true,
            Some(b'/' | b'.') => digit,
            _ => false,
        };
        let found = if shaped { shape(self.bytes, at) } else { None };
        if let Some((len, role)) = found {
            self.emit(at, at + len, role);
            self.index = at + len;
            return;
        }
        let hex = size == 8 || size == 16;
        let number = (digit || (hex && first.is_ascii_hexdigit()))
            .then(|| number_end(self.bytes, at, run))
            .flatten();
        if let Some(end) = number {
            self.emit(at, end, "number");
            self.index = end;
            return;
        }
        let end = word_end(self.bytes, run);
        if let Some(role) = level(&self.chunk[at..end], self.bytes.get(end).copied()) {
            self.emit(at, end, role);
        }
        self.index = end;
    }

    /// Colours a path, GUID or signed number that starts at an opener byte.
    fn opener(&mut self, at: usize, first: u8) {
        if let Some((len, role)) = shape(self.bytes, at) {
            self.emit(at, at + len, role);
            self.index = at + len;
        } else if first == b'-'
            && let Some(end) = number_end(self.bytes, at, at)
        {
            self.emit(at, end, "number");
            self.index = end;
        } else {
            self.index += 1;
        }
    }

    /// Skips an unresolved token: stops after whitespace or before an allowed quote.
    fn skip(&mut self) {
        while let Some(&byte) = self.bytes.get(self.index) {
            if (byte == b'"' || byte == b'\'') && !self.prev_word(self.index) {
                self.mode = NORMAL;
                return;
            }
            self.index += 1;
            if byte.is_ascii_whitespace() {
                self.mode = NORMAL;
                return;
            }
        }
    }

    /// Scans to the closing double quote or the chunk end and colours it as one string.
    fn double_tail(&mut self, start: usize) {
        let mut escape = self.flags & ESCAPE != 0;
        while let Some(&byte) = self.bytes.get(self.index) {
            self.index += 1;
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                self.mode = NORMAL;
                break;
            }
        }
        self.flags = if escape { ESCAPE } else { 0 };
        self.emit(start, self.index, "string");
    }

    /// Looks for the close of a single-quoted candidate that began at `start`.
    ///
    /// A close is a `'` followed by a non-word byte, within `SINGLE_MAX` bytes of the open and
    /// before any `"`. `carried` means the candidate began in an earlier chunk.
    fn single_scan(&mut self, start: usize, carried: bool) {
        self.mode = IN_SINGLE;
        if carried && self.flags & PENDING != 0 {
            self.flags &= !PENDING;
            if !is_word(self.bytes[self.index]) {
                self.mode = NORMAL;
                return;
            }
        }
        while self.count < SINGLE_MAX {
            let Some(&byte) = self.bytes.get(self.index) else {
                return self.single_open(start, carried);
            };
            if byte == b'"' {
                return self.single_fail(start, carried);
            }
            if byte == b'\'' && self.single_quote(start) {
                return;
            }
            self.index += 1;
            self.count += 1;
        }
        self.mode = SKIP;
    }

    /// Handles a `'` inside a candidate and returns true when it closed the string.
    fn single_quote(&mut self, start: usize) -> bool {
        match self.quote_follow() {
            Follow::Boundary => self.single_close(start),
            Follow::Unknown => self.flags |= PENDING,
            Follow::Word => {}
        }
        self.mode == NORMAL
    }

    /// Classifies the byte after the `'` at the current index.
    fn quote_follow(&self) -> Follow {
        match self.bytes.get(self.index + 1) {
            Some(&next) if !is_word(next) => Follow::Boundary,
            None if self.last => Follow::Boundary,
            None => Follow::Unknown,
            Some(_) => Follow::Word,
        }
    }

    /// Colours a closed single-quoted string that began at `start`.
    fn single_close(&mut self, start: usize) {
        self.index += 1;
        self.mode = NORMAL;
        self.emit(start, self.index, "string.special");
    }

    /// Handles a candidate that reached the chunk end without a close.
    fn single_open(&mut self, start: usize, carried: bool) {
        if self.last {
            self.single_fail(start, carried);
        } else {
            self.flags &= PENDING;
        }
    }

    /// Drops a failed candidate: a fresh one resumes after its quote, a carried one stays put.
    fn single_fail(&mut self, start: usize, carried: bool) {
        self.mode = NORMAL;
        if carried {
            self.flags &= PREV_WORD;
        } else {
            self.flags = 0;
            self.index = start + 1;
        }
    }
}

use std::ops::Range;

use crate::{Delimiter, Inside, MAX_PATTERN_BYTES, Token, TokenKind};

pub(super) enum Step {
    At(usize),
    Done(Inside),
}

/// Where a delimiter's close search ended.
enum Close {
    /// The close ends at this byte.
    At(usize),
    /// No close in the text; `escaped` when the text ends on an escape char, and `partial` bytes
    /// of the close end the text (only looked for when asked to).
    Open { escaped: bool, partial: usize },
}

pub(super) struct Run<'a> {
    pub(super) line: &'a str,
    pub(super) tokens: Vec<Token>,
    /// True when no later chunk continues the line.
    pub(super) last: bool,
    /// Bytes of a closer that end the text, when the text ends inside a block or string.
    pub(super) closing: usize,
    /// Most tokens to keep; more than [`MAX_TOKENS`] when held-back bytes come first.
    pub(super) cap: usize,
    /// Where the last bytes that an opener may still be starting begin (chunks that may continue).
    pub(super) tail_from: usize,
    /// Every step start at or after `tail_from`, in order (chunks that may continue).
    pub(super) marks: Vec<usize>,
}

/// Most tokens one line may keep; later tokens are scanned but not stored.
pub const MAX_TOKENS: usize = 4096;

impl<'a> Run<'a> {
    pub(super) fn new(line: &'a str, last: bool) -> Self {
        Self {
            line,
            tokens: Vec::new(),
            last,
            closing: 0,
            cap: MAX_TOKENS,
            tail_from: 0,
            marks: Vec::new(),
        }
    }

    /// Stores the token at `start..end`, unless the run already has `cap` tokens.
    pub(super) fn push(&mut self, start: usize, end: usize, kind: TokenKind) {
        if end > start && self.tokens.len() < self.cap {
            self.tokens.push(Token {
                range: start..end,
                kind,
            });
        }
    }

    pub(super) fn block(&mut self, opened: Range<usize>, close: &str, index: Option<u16>) -> Step {
        match find_from(self.line, opened.end, close) {
            Some(end) => {
                self.push(opened.start, end, TokenKind::Comment);
                Step::At(end)
            }
            None => {
                self.push(opened.start, self.line.len(), TokenKind::Comment);
                if !self.last && close.len() <= MAX_PATTERN_BYTES {
                    self.closing = partial_len(self.line.get(opened.end..), close);
                }
                Step::Done(index.map_or(Inside::Code, Inside::Block))
            }
        }
    }

    pub(super) fn delimited(
        &mut self,
        opened: Range<usize>,
        delimiter: &Delimiter,
        index: Option<u16>,
    ) -> Step {
        match close_of(
            self.line,
            opened.end,
            delimiter,
            !self.last && delimiter.close.len() <= MAX_PATTERN_BYTES,
        ) {
            Close::At(end) => {
                self.push(opened.start, end, delimiter.kind);
                Step::At(end)
            }
            Close::Open { escaped, partial } => {
                self.push(opened.start, self.line.len(), delimiter.kind);
                self.closing = partial;
                Step::Done(self.carried(delimiter, index, escaped))
            }
        }
    }

    /// What a text that ends inside `delimiter` hands on: a chunk that may continue keeps every
    /// string; the end of a line keeps only a multiline one, and an escape ends with the line.
    pub(super) fn carried(
        &self,
        delimiter: &Delimiter,
        index: Option<u16>,
        escaped: bool,
    ) -> Inside {
        let multiline = delimiter.multiline && !delimiter.close.is_empty();
        match index {
            Some(index) if !self.last => Inside::Delimited { index, escaped },
            Some(index) if multiline => Inside::Delimited {
                index,
                escaped: false,
            },
            _ => Inside::Code,
        }
    }
}

fn find_from(line: &str, from: usize, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    line.get(from..)?
        .find(needle)
        .map(|found| from + found + needle.len())
}

/// How many bytes of `close` (fewer than all of it) end `text`; the most is the leftmost start.
fn partial_len(text: Option<&str>, close: &str) -> usize {
    let Some(text) = text else { return 0 };
    (1..close.len())
        .rev()
        .find(|&len| close.get(..len).is_some_and(|head| text.ends_with(head)))
        .unwrap_or(0)
}

/// Finds the close of `delimiter` from byte `from`. With `track`, an open end also reports how
/// many bytes of the close end the text.
fn close_of(line: &str, from: usize, delimiter: &Delimiter, track: bool) -> Close {
    let none = Close::Open {
        escaped: false,
        partial: 0,
    };
    let Some(rest) = line.get(from..).filter(|_| !delimiter.close.is_empty()) else {
        return none;
    };
    let close = delimiter.close.as_str();
    let mut partial = 0;
    let mut chars = rest.char_indices();
    while let Some((offset, ch)) = chars.next() {
        if Some(ch) == delimiter.escape {
            if chars.next().is_none() {
                // A close the text may be starting before the escape comes first (it ends there).
                return Close::Open {
                    escaped: partial == 0,
                    partial,
                };
            }
        } else if rest[offset..].starts_with(close) {
            return Close::At(from + offset + close.len());
        } else if track
            && partial == 0
            && rest.len() - offset < close.len()
            && close.starts_with(&rest[offset..])
        {
            partial = rest.len() - offset;
        }
    }
    Close::Open {
        escaped: false,
        partial,
    }
}

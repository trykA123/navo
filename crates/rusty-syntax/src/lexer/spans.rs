use std::ops::Range;

use crate::{Delimiter, LineState, Token, TokenKind};

pub(super) enum Step {
    At(usize),
    Done(LineState),
}

pub(super) struct Run<'a> {
    pub(super) line: &'a str,
    pub(super) tokens: Vec<Token>,
}

impl<'a> Run<'a> {
    pub(super) fn new(line: &'a str) -> Self {
        Self {
            line,
            tokens: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, start: usize, end: usize, kind: TokenKind) {
        if end > start {
            self.tokens.push(Token {
                range: start..end,
                kind,
            });
        }
    }

    pub(super) fn block(&mut self, opened: Range<usize>, close: &str, index: u8) -> Step {
        match find_from(self.line, opened.end, close) {
            Some(end) => {
                self.push(opened.start, end, TokenKind::Comment);
                Step::At(end)
            }
            None => {
                self.push(opened.start, self.line.len(), TokenKind::Comment);
                Step::Done(LineState::Block(index))
            }
        }
    }

    pub(super) fn delimited(
        &mut self,
        opened: Range<usize>,
        delimiter: &Delimiter,
        index: u8,
    ) -> Step {
        match close_of(self.line, opened.end, delimiter) {
            Some(end) => {
                self.push(opened.start, end, delimiter.kind);
                Step::At(end)
            }
            None => {
                self.push(opened.start, self.line.len(), delimiter.kind);
                let carried = delimiter.multiline && !delimiter.close.is_empty();
                Step::Done(if carried {
                    LineState::Delimited(index)
                } else {
                    LineState::Normal
                })
            }
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

fn close_of(line: &str, from: usize, delimiter: &Delimiter) -> Option<usize> {
    if delimiter.close.is_empty() {
        return None;
    }
    let rest = line.get(from..)?;
    let mut chars = rest.char_indices();
    while let Some((offset, ch)) = chars.next() {
        if Some(ch) == delimiter.escape {
            chars.next();
        } else if rest[offset..].starts_with(&delimiter.close) {
            return Some(from + offset + delimiter.close.len());
        }
    }
    None
}

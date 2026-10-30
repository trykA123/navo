use std::ops::Range;
use std::time::{Duration, Instant};

use rusty_core::paging::TextLayout;
use similar::{Algorithm, DiffTag, TextDiff};

use super::side::Line;

/// D7: a line over 4,096 characters is a whole-line change.
pub const INTRALINE_MAX_CHARS: usize = 4_096;
/// Changes inside a line are refined to characters only on short lines.
pub const INTRALINE_SHORT_CHARS: usize = 80;
/// D20: one call past this deadline falls back to the whole line.
pub const INTRALINE_DEADLINE: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub side: SpanSide,
    /// Character offsets into the decoded line.
    pub chars: Range<usize>,
    /// Source byte offsets from the file start, including the side's encoding.
    pub source: Range<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntralineFallback {
    LineTooLong,
    Lossy,
    Deadline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intraline {
    pub words: Vec<Span>,
    pub chars: Vec<Span>,
    pub fallback: Option<IntralineFallback>,
}

#[derive(Debug, Clone, Copy)]
pub struct SideLine<'a> {
    pub line: &'a Line,
    pub start: u64,
    pub layout: TextLayout,
    pub lossless: bool,
}

impl SideLine<'_> {
    /// Map decoded character offsets to source byte offsets.
    #[must_use]
    pub fn source_range(&self, chars: Range<usize>) -> Range<u64> {
        let text = self.line.text.as_str();
        let start_byte = byte_of_char(text, chars.start);
        let end_byte = byte_of_char(text, chars.end);
        let prefix = self
            .layout
            .encode(&text[..start_byte])
            .map_or(start_byte, |bytes| bytes.len());
        let body = self
            .layout
            .encode(&text[start_byte..end_byte])
            .map_or(end_byte - start_byte, |bytes| bytes.len());
        let start = self.start.saturating_add(prefix as u64);
        start..start.saturating_add(body as u64)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinePair<'a> {
    pub left: SideLine<'a>,
    pub right: SideLine<'a>,
}

#[must_use]
pub fn intraline(pair: LinePair<'_>) -> Intraline {
    intraline_with_deadline(pair, INTRALINE_DEADLINE)
}

fn intraline_with_deadline(pair: LinePair<'_>, budget: Duration) -> Intraline {
    if !pair.left.lossless || !pair.right.lossless {
        return fallback(IntralineFallback::Lossy);
    }
    let left_chars = pair.left.line.text.chars().count();
    let right_chars = pair.right.line.text.chars().count();
    if left_chars > INTRALINE_MAX_CHARS || right_chars > INTRALINE_MAX_CHARS {
        return fallback(IntralineFallback::LineTooLong);
    }
    let deadline = Instant::now() + budget;
    let mut config = TextDiff::configure();
    config.algorithm(Algorithm::Myers);
    config.deadline(deadline);
    let words = config.diff_words(&pair.left.line.text, &pair.right.line.text);
    if Instant::now() >= deadline {
        return fallback(IntralineFallback::Deadline);
    }
    let word_spans = word_spans(&pair, &words);
    if Instant::now() >= deadline {
        return fallback(IntralineFallback::Deadline);
    }
    if word_spans.is_empty() {
        return Intraline {
            words: Vec::new(),
            chars: Vec::new(),
            fallback: None,
        };
    }
    if left_chars > INTRALINE_SHORT_CHARS || right_chars > INTRALINE_SHORT_CHARS {
        return Intraline {
            words: word_spans,
            chars: Vec::new(),
            fallback: None,
        };
    }
    let mut config = TextDiff::configure();
    config.algorithm(Algorithm::Myers);
    config.deadline(deadline);
    let chars = config.diff_chars(&pair.left.line.text, &pair.right.line.text);
    if Instant::now() >= deadline {
        return fallback(IntralineFallback::Deadline);
    }
    let char_spans = char_spans(&pair, &chars);
    if Instant::now() >= deadline {
        return fallback(IntralineFallback::Deadline);
    }
    Intraline {
        words: word_spans,
        chars: char_spans,
        fallback: None,
    }
}

fn word_spans(pair: &LinePair<'_>, diff: &TextDiff<'_, '_, str>) -> Vec<Span> {
    let left = WordSide {
        side: SpanSide::Left,
        line: &pair.left,
        starts: token_starts(diff.iter_old_slices()),
    };
    let right = WordSide {
        side: SpanSide::Right,
        line: &pair.right,
        starts: token_starts(diff.iter_new_slices()),
    };
    let mut spans = Vec::new();
    for op in diff.ops() {
        let (tag, old, new) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        add(&mut spans, &left, old);
        add(&mut spans, &right, new);
    }
    spans
}

struct WordSide<'line, 'data> {
    side: SpanSide,
    line: &'line SideLine<'data>,
    starts: Vec<usize>,
}

fn char_spans(pair: &LinePair<'_>, diff: &TextDiff<'_, '_, str>) -> Vec<Span> {
    let mut spans = Vec::new();
    for op in diff.ops() {
        let (tag, old, new) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        if old.start < old.end {
            spans.push(span(SpanSide::Left, &pair.left, old));
        }
        if new.start < new.end {
            spans.push(span(SpanSide::Right, &pair.right, new));
        }
    }
    spans
}

fn add(spans: &mut Vec<Span>, target: &WordSide<'_, '_>, range: Range<usize>) {
    if range.start >= range.end {
        return;
    }
    let chars = target.starts[range.start]..target.starts[range.end];
    spans.push(span(target.side, target.line, chars));
}

fn span(side: SpanSide, line: &SideLine<'_>, chars: Range<usize>) -> Span {
    Span {
        side,
        source: line.source_range(chars.clone()),
        chars,
    }
}

fn token_starts<'a>(tokens: impl Iterator<Item = &'a str>) -> Vec<usize> {
    let mut starts = vec![0];
    let mut at = 0;
    for token in tokens {
        at += token.chars().count();
        starts.push(at);
    }
    starts
}

fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(offset, _)| offset)
}

fn fallback(kind: IntralineFallback) -> Intraline {
    Intraline {
        words: Vec::new(),
        chars: Vec::new(),
        fallback: Some(kind),
    }
}

#[cfg(test)]
mod tests;

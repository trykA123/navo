mod chunking;
mod holds;
mod scan;
mod spans;
mod trie;
mod words;

use std::cmp::Reverse;

use crate::{
    ChunkState, Delimiter, Inside, KeywordGroup, Language, LineState, MAX_PATTERN_BYTES, Token,
    TokenKind,
};
use holds::Holds;
use scan::{after_word, is_word, number_len, starts_with, word_before, word_len};
use spans::{Run, Step};
use trie::Trie;
use words::Words;

#[must_use]
pub fn lex_line(lang: &Language, line: &str, state: LineState) -> (Vec<Token>, LineState) {
    Lexer::new(lang).line(line, state)
}

/// What a token starting at a byte can be; lets `scan` skip dead bytes and shortcut plain words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteClass {
    /// ASCII byte that starts nothing: skipped without a probe.
    Dead,
    /// Word byte that no opener, special word or number starts with: only a keyword can match.
    Word,
    /// Anything else: run the full probe.
    Full,
}

enum Opener {
    Line,
    Block(usize),
}

#[derive(Debug, Clone)]
pub struct Lexer {
    words: Words,
    classes: [ByteClass; 256],
    special: Trie<(TokenKind, bool)>,
    line_comments: Vec<String>,
    blocks: Vec<(String, String)>,
    delimiters: Vec<Delimiter>,
    delimiter_opens: Trie<usize>,
    /// Every pattern a chunk end can cut, in the order the chunk state indexes them.
    holds: Holds,
    operators: Trie<()>,
    numbers: bool,
}

/// True when a pattern is short enough for the chunk state to carry a cut of it.
fn fits(pattern: &str) -> bool {
    pattern.len() <= MAX_PATTERN_BYTES
}

fn longest_first(items: &[String]) -> Vec<String> {
    let mut items: Vec<String> = items
        .iter()
        .filter(|item| !item.is_empty() && fits(item))
        .cloned()
        .collect();
    items.sort_by_key(|item| Reverse(item.len()));
    items
}

fn exact<'a, T: Copy>(items: impl IntoIterator<Item = (&'a String, T)>) -> Trie<T> {
    let mut trie = Trie::new(false);
    for (item, value) in items {
        trie.insert(item, value);
    }
    trie
}

fn plain(group: &KeywordGroup, word: &str) -> bool {
    !group.prefix && word_len(word) == word.len()
}

impl Lexer {
    /// Builds a lexer for `lang`. Like [`Language::check_limits`], it ignores every pattern over
    /// [`MAX_PATTERN_BYTES`]: a comment, string, operator or special keyword that long never
    /// matches, so a chunk end never cuts more than that many bytes of one. Plain words are not
    /// limited.
    #[must_use]
    pub fn new(lang: &Language) -> Self {
        let entries = lang.keywords.iter().flat_map(|group| {
            group
                .words
                .iter()
                .filter(|word| !word.is_empty())
                .map(move |word| (group, word))
        });
        let (plain_words, special_words): (Vec<_>, Vec<_>) =
            entries.partition(|(group, word)| plain(group, word));
        let special_words: Vec<_> = special_words
            .into_iter()
            .filter(|(_, word)| fits(word))
            .collect();
        let specials: Vec<&String> = special_words.iter().map(|(_, word)| *word).collect();
        let operators: Vec<&String> = lang.operators.iter().filter(|op| fits(op)).collect();
        let blocks: Vec<_> = (lang.block_comments.iter())
            .filter(|(open, close)| fits(open) && fits(close))
            .cloned()
            .collect();
        let delimiters: Vec<Delimiter> = (lang.delimiters.iter())
            .filter(|d| fits(&d.open) && fits(&d.close))
            .cloned()
            .collect();
        let delimiter_opens = exact(delimiters.iter().map(|d| &d.open).zip(0..));
        let words = Words::new(
            lang.case_ignored,
            plain_words
                .into_iter()
                .map(|(group, word)| (word, group.kind)),
        );
        let mut special = Trie::new(lang.case_ignored);
        for (group, word) in special_words {
            special.insert(word, (group.kind, group.prefix));
        }
        let mut lexer = Self {
            words,
            classes: [ByteClass::Full; 256],
            special,
            line_comments: longest_first(&lang.line_comments),
            blocks,
            delimiters,
            delimiter_opens,
            operators: exact(operators.iter().map(|op| (*op, ()))),
            holds: Holds::default(),
            numbers: lang.numbers,
        };
        lexer.holds = lexer.hold_list(&operators, &specials, lang.case_ignored);
        lexer.classes = lexer.byte_classes();
        lexer
    }

    /// Lists the patterns a chunk end can cut in two: comment and string openers, operators and
    /// special keywords.
    fn hold_list(&self, operators: &[&String], specials: &[&String], folded: bool) -> Holds {
        let blocks = self.blocks.iter().map(|(open, _)| open);
        let strings = self.delimiters.iter().map(|d| &d.open);
        let openers: Vec<&String> = (self.line_comments.iter())
            .chain(blocks)
            .chain(strings)
            .collect();
        Holds::new(&openers, operators, specials, folded)
    }

    /// Sorts each byte by what a token starting there can be, so `scan` can skip or shortcut.
    fn byte_classes(&self) -> [ByteClass; 256] {
        let mut classes = [ByteClass::Full; 256];
        for (class, byte) in classes.iter_mut().zip(0..=u8::MAX) {
            let comment = self
                .line_comments
                .iter()
                .chain(self.blocks.iter().map(|(open, _)| open))
                .any(|open| open.as_bytes().first() == Some(&byte));
            let opener = comment
                || self.delimiter_opens.starts_with_byte(byte)
                || self.operators.starts_with_byte(byte)
                || self.special.starts_with_byte(byte);
            let word = byte.is_ascii_alphanumeric() || byte == b'_';
            if byte.is_ascii() && !opener && !word {
                *class = ByteClass::Dead;
            } else if word && !opener && !(self.numbers && byte.is_ascii_digit()) {
                *class = ByteClass::Word;
            }
        }
        classes
    }

    /// Lexes one whole line that starts in `state`; returns its tokens and the state at its end.
    #[must_use]
    pub fn line(&self, line: &str, state: LineState) -> (Vec<Token>, LineState) {
        let feed = chunking::Feed {
            last: true,
            lead: 0,
            held: 0,
        };
        let (tokens, next) = self.lex(line, ChunkState::from_line(state), feed);
        (tokens, next.line_state())
    }

    /// Starts a chunk in the state the chunk before left: inside a comment or string, or code.
    fn resume(&self, run: &mut Run<'_>, state: ChunkState) -> Step {
        match state.inside {
            Inside::Code if state.mid_word => self.leave_word(run),
            Inside::Code | Inside::Opening(_) => Step::At(0),
            Inside::LineComment => {
                run.push(0, run.line.len(), TokenKind::Comment);
                Step::Done(if run.last {
                    Inside::Code
                } else {
                    Inside::LineComment
                })
            }
            Inside::Block(index) => match self.blocks.get(usize::from(index)) {
                Some((_, close)) => run.block(0..0, close, Some(index)),
                None => Step::At(0),
            },
            Inside::Delimited { index, escaped } => self.resume_string(run, index, escaped),
        }
    }

    /// Continues string `index`; a pending escape char makes the chunk's first char part of it.
    fn resume_string(&self, run: &mut Run<'_>, index: u16, escaped: bool) -> Step {
        let Some(delimiter) = self.delimiters.get(usize::from(index)) else {
            return Step::At(0);
        };
        let first = run.line.chars().next().map_or(0, char::len_utf8);
        if escaped && first == 0 {
            return Step::Done(run.carried(delimiter, Some(index), true));
        }
        run.delimited(0..if escaped { first } else { 0 }, delimiter, Some(index))
    }

    /// Starts a chunk whose first byte continues a word: skips that word, or lexes a first
    /// non-word byte as inside a word (no word boundary before it).
    fn leave_word(&self, run: &mut Run<'_>) -> Step {
        let skip = word_len(run.line);
        if skip > 0 {
            return Step::At(skip);
        }
        let first = run.line.chars().next().map_or(0, char::len_utf8);
        if first == 0 {
            return Step::At(0);
        }
        self.span_at(run, 0, false).unwrap_or(Step::At(first))
    }

    /// Lexes the chunk from byte `at`; with `TRACK` it also notes every step start near the end,
    /// where an opener may still be starting. Dead bytes are skipped in bulk and plain words take a
    /// shortcut; every other position runs the full probe. Returns where the chunk stops.
    fn scan<const TRACK: bool>(&self, run: &mut Run<'_>, mut at: usize) -> Inside {
        let line = run.line;
        while let Some(&byte) = line.as_bytes().get(at) {
            if TRACK && at >= run.tail_from {
                run.marks.push(at);
            }
            let class = self.classes[usize::from(byte)];
            if class == ByteClass::Dead {
                at += 1;
                continue;
            }
            let boundary = !word_before(line, at);
            if class == ByteClass::Word && boundary {
                at += self.plain_word(run, at);
                continue;
            }
            match self.span_at(run, at, boundary) {
                Some(Step::At(end)) => at = end,
                Some(Step::Done(next)) => return next,
                None => {
                    let rest = line.get(at..).unwrap_or_default();
                    at += word_len(rest).max(rest.chars().next().map_or(1, char::len_utf8));
                }
            }
        }
        Inside::Code
    }

    /// Stores the keyword at `at`, if the word there is one, and returns the word's byte length.
    /// Only for a word start whose byte class is `Word`, so the length is never zero.
    fn plain_word(&self, run: &mut Run<'_>, at: usize) -> usize {
        let (len, kind) = self.words.find(run.line.get(at..).unwrap_or_default());
        if let Some(kind) = kind {
            run.push(at, at + len, kind);
        }
        len
    }

    /// Probes for a token at `at`; `boundary` is false when a word char precedes the byte.
    fn span_at(&self, run: &mut Run<'_>, at: usize, boundary: bool) -> Option<Step> {
        let line = run.line;
        let rest = &line[at..];
        let index = |i: usize| u16::try_from(i).ok();
        match self.comment_at(rest, boundary) {
            Some(Opener::Line) => {
                run.push(at, run.line.len(), TokenKind::Comment);
                let ends = if run.last {
                    Inside::Code
                } else {
                    Inside::LineComment
                };
                return Some(Step::Done(ends));
            }
            Some(Opener::Block(i)) => {
                let (open, close) = &self.blocks[i];
                return Some(run.block(at..at + open.len(), close, index(i)));
            }
            None => {}
        }
        let may_open = boundary || !rest.chars().next().is_some_and(is_word);
        let delimiter = self
            .delimiter_opens
            .matches(rest)
            .filter(|_| may_open)
            .map(|(_, i)| i)
            .min();
        if let Some(i) = delimiter {
            let delimiter = &self.delimiters[i];
            let opened = at..at + delimiter.open.len();
            return Some(run.delimited(opened, delimiter, index(i)));
        }
        let (len, kind) = self.token_at(rest, boundary, may_open)?;
        run.push(at, at + len, kind);
        Some(Step::At(at + len))
    }

    fn comment_at(&self, rest: &str, boundary: bool) -> Option<Opener> {
        let line = self
            .line_comments
            .iter()
            .find(|c| self.opens(rest, c, boundary))
            .map(String::len);
        let block = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, (open, _))| self.opens(rest, open, boundary))
            .min_by_key(|(_, (open, _))| Reverse(open.len()))
            .map(|(i, (open, _))| (i, open.len()));
        match (line, block) {
            (Some(line), Some((i, block))) if block > line => Some(Opener::Block(i)),
            (Some(_), _) => Some(Opener::Line),
            (None, block) => block.map(|(i, _)| Opener::Block(i)),
        }
    }

    fn token_at(&self, rest: &str, boundary: bool, may_open: bool) -> Option<(usize, TokenKind)> {
        if boundary {
            if let Some(len) = number_len(rest).filter(|_| self.numbers) {
                return Some((len, TokenKind::Number));
            }
            if let Some(found) = self.keyword(rest) {
                return Some(found);
            }
        }
        self.operators
            .matches(rest)
            .filter(|_| may_open)
            .find_map(|(len, ())| {
                let op = rest.get(..len)?;
                after_word(rest, len, op).then_some((len, TokenKind::Operator))
            })
    }

    fn keyword(&self, rest: &str) -> Option<(usize, TokenKind)> {
        let (len, found) = self.words.find(rest);
        if let Some(kind) = found {
            return Some((len, kind));
        }
        self.special
            .matches(rest)
            .find_map(|(len, (kind, prefix))| {
                let (word, tail) = rest.split_at_checked(len)?;
                if prefix {
                    return Some((len + word_len(tail), kind));
                }
                after_word(rest, len, word).then_some((len, kind))
            })
    }

    fn opens(&self, rest: &str, open: &str, boundary: bool) -> bool {
        !open.is_empty()
            && (boundary || !open.chars().next().is_some_and(is_word))
            && starts_with(rest, open, false)
    }
}

#[cfg(test)]
mod tests;

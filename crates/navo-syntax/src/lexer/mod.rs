mod scan;
mod spans;
mod trie;

use std::cmp::Reverse;
use std::collections::HashMap;

use crate::{Delimiter, KeywordGroup, Language, LineState, Token, TokenKind};
use scan::{after_word, is_word, number_len, starts_with, word_len};
use spans::{Run, Step};
use trie::Trie;

#[must_use]
pub fn lex_line(lang: &Language, line: &str, state: LineState) -> (Vec<Token>, LineState) {
    Lexer::new(lang).line(line, state)
}

enum Opener {
    Line,
    Block(usize),
}

#[derive(Debug, Clone)]
pub struct Lexer {
    case_ignored: bool,
    words: HashMap<String, TokenKind>,
    special: Trie<(TokenKind, bool)>,
    line_comments: Vec<String>,
    blocks: Vec<(String, String)>,
    delimiters: Vec<Delimiter>,
    delimiter_opens: Trie<usize>,
    operators: Trie<()>,
    numbers: bool,
}

fn longest_first(items: &[String]) -> Vec<String> {
    let mut items: Vec<String> = items
        .iter()
        .filter(|item| !item.is_empty())
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
        let fold = |word: &String| {
            if lang.case_ignored {
                word.to_lowercase()
            } else {
                word.clone()
            }
        };
        let mut words = HashMap::new();
        for (group, word) in plain_words {
            words.entry(fold(word)).or_insert(group.kind);
        }
        let mut special = Trie::new(lang.case_ignored);
        for (group, word) in special_words {
            special.insert(word, (group.kind, group.prefix));
        }
        Self {
            case_ignored: lang.case_ignored,
            words,
            special,
            line_comments: longest_first(&lang.line_comments),
            blocks: lang.block_comments.clone(),
            delimiters: lang.delimiters.clone(),
            delimiter_opens: exact(lang.delimiters.iter().map(|d| &d.open).zip(0..)),
            operators: exact(lang.operators.iter().map(|op| (op, ()))),
            numbers: lang.numbers,
        }
    }

    #[must_use]
    pub fn line(&self, line: &str, state: LineState) -> (Vec<Token>, LineState) {
        let mut run = Run::new(line);
        let next = match self.resume(&mut run, state) {
            Step::At(at) => self.scan(&mut run, at),
            Step::Done(next) => next,
        };
        (run.tokens, next)
    }

    fn resume(&self, run: &mut Run<'_>, state: LineState) -> Step {
        match state {
            LineState::Block(index) => match self.blocks.get(usize::from(index)) {
                Some((_, close)) => run.block(0..0, close, Some(index)),
                None => Step::At(0),
            },
            LineState::Delimited(index) => match self.delimiters.get(usize::from(index)) {
                Some(delimiter) => run.delimited(0..0, delimiter, Some(index)),
                None => Step::At(0),
            },
            LineState::Normal => Step::At(0),
        }
    }

    fn scan(&self, run: &mut Run<'_>, mut at: usize) -> LineState {
        let line = run.line;
        while let Some(rest) = line.get(at..).filter(|rest| !rest.is_empty()) {
            match self.span_at(run, at) {
                Some(Step::At(end)) => at = end,
                Some(Step::Done(next)) => return next,
                None => at += word_len(rest).max(rest.chars().next().map_or(1, char::len_utf8)),
            }
        }
        LineState::Normal
    }

    fn span_at(&self, run: &mut Run<'_>, at: usize) -> Option<Step> {
        let line = run.line;
        let rest = &line[at..];
        let boundary = !line[..at].chars().next_back().is_some_and(is_word);
        let index = |i: usize| u16::try_from(i).ok();
        match self.comment_at(rest, boundary) {
            Some(Opener::Line) => {
                run.push(at, run.line.len(), TokenKind::Comment);
                return Some(Step::Done(LineState::Normal));
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
        let len = word_len(rest);
        let word = &rest[..len];
        let found = if self.case_ignored {
            self.words.get(&word.to_lowercase())
        } else {
            self.words.get(word)
        };
        if let Some(kind) = found.filter(|_| len > 0) {
            return Some((len, *kind));
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

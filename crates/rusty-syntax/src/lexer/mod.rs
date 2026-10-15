mod scan;
mod spans;

use std::collections::HashMap;

use crate::{Delimiter, KeywordGroup, Language, LineState, Token, TokenKind};
use scan::{after_word, is_word, number_len, starts_with, word_len};
use spans::{Run, Step};

#[must_use]
pub fn lex_line(lang: &Language, line: &str, state: LineState) -> (Vec<Token>, LineState) {
    Lexer::new(lang).line(line, state)
}

#[derive(Debug, Clone)]
pub struct Lexer {
    case_ignored: bool,
    words: HashMap<String, TokenKind>,
    special: Vec<(String, TokenKind, bool)>,
    line_comments: Vec<String>,
    blocks: Vec<(String, String)>,
    delimiters: Vec<Delimiter>,
    operators: Vec<String>,
    numbers: bool,
}

fn longest_first(items: &[String]) -> Vec<String> {
    let mut items: Vec<String> = items
        .iter()
        .filter(|item| !item.is_empty())
        .cloned()
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.len()));
    items
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
        let mut special: Vec<(String, TokenKind, bool)> = special_words
            .into_iter()
            .map(|(group, word)| (word.clone(), group.kind, group.prefix))
            .collect();
        special.sort_by_key(|(word, _, _)| std::cmp::Reverse(word.len()));
        Self {
            case_ignored: lang.case_ignored,
            words,
            special,
            line_comments: longest_first(&lang.line_comments),
            blocks: lang.block_comments.clone(),
            delimiters: lang.delimiters.clone(),
            operators: longest_first(&lang.operators),
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
                Some((_, close)) => run.block(0..0, close, index),
                None => Step::At(0),
            },
            LineState::Delimited(index) => match self.delimiters.get(usize::from(index)) {
                Some(delimiter) => run.delimited(0..0, delimiter, index),
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
        if self
            .line_comments
            .iter()
            .any(|c| self.opens(rest, c, boundary))
        {
            run.push(at, run.line.len(), TokenKind::Comment);
            return Some(Step::Done(LineState::Normal));
        }
        let index = |i: usize| u8::try_from(i).unwrap_or(u8::MAX);
        if let Some(i) = self
            .blocks
            .iter()
            .position(|(open, _)| self.opens(rest, open, boundary))
        {
            let (open, close) = &self.blocks[i];
            return Some(run.block(at..at + open.len(), close, index(i)));
        }
        if let Some(i) = self
            .delimiters
            .iter()
            .position(|d| self.opens(rest, &d.open, boundary))
        {
            let delimiter = &self.delimiters[i];
            let opened = at..at + delimiter.open.len();
            return Some(run.delimited(opened, delimiter, index(i)));
        }
        let (len, kind) = self.token_at(rest, boundary)?;
        run.push(at, at + len, kind);
        Some(Step::At(at + len))
    }

    fn token_at(&self, rest: &str, boundary: bool) -> Option<(usize, TokenKind)> {
        if boundary {
            if let Some(len) = number_len(rest).filter(|_| self.numbers) {
                return Some((len, TokenKind::Number));
            }
            if let Some(found) = self.keyword(rest) {
                return Some(found);
            }
        }
        self.operators
            .iter()
            .find(|op| self.opens(rest, op, boundary) && after_word(rest, op.len(), op))
            .map(|op| (op.len(), TokenKind::Operator))
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
        self.special.iter().find_map(|(word, kind, prefix)| {
            if !starts_with(rest, word, self.case_ignored) {
                return None;
            }
            if *prefix {
                return Some((word.len() + word_len(&rest[word.len()..]), *kind));
            }
            after_word(rest, word.len(), word).then_some((word.len(), *kind))
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

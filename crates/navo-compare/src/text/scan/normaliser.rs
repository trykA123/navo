use xxhash_rust::xxh3::Xxh3;

use crate::model::CaseRule;

use super::super::normalise::{Options, Whitespace};
use super::super::side::LineEnding;

pub(super) struct Normaliser {
    case: CaseRule,
    whitespace: Whitespace,
    ignore_line_endings: bool,
    hasher: Xxh3,
    pending_space: Option<Xxh3>,
    collapsing: bool,
    last: Option<char>,
    nonblank: usize,
}

impl Normaliser {
    pub(super) fn new(options: &Options, bom: bool) -> Self {
        let mut hasher = Xxh3::new();
        let mut nonblank = 0;
        if bom && !options.ignore_bom {
            update(&mut hasher, '\u{feff}');
            nonblank += '\u{feff}'.len_utf8();
        }
        Self {
            case: options.case,
            whitespace: options.whitespace,
            ignore_line_endings: options.ignore_line_endings,
            hasher,
            pending_space: None,
            collapsing: false,
            last: None,
            nonblank,
        }
    }

    pub(super) fn push(&mut self, character: char) {
        if let Some(previous) = self.last.replace(character) {
            self.emit(previous);
        }
    }

    fn emit(&mut self, character: char) {
        match self.case {
            CaseRule::Sensitive => self.text(character),
            CaseRule::Insensitive => self.fold_insensitive(character),
        }
    }

    fn fold_insensitive(&mut self, character: char) {
        for upper in character.to_uppercase() {
            for lower in upper.to_lowercase() {
                self.text(lower);
            }
        }
    }

    fn text(&mut self, character: char) {
        match self.whitespace {
            Whitespace::Keep => self.put(character),
            Whitespace::All => self.put_visible(character),
            Whitespace::Changes => self.put_collapsed(character),
            Whitespace::Trailing => self.put_trimmed(character),
        }
    }

    fn put_visible(&mut self, character: char) {
        if !character.is_whitespace() {
            self.put(character);
        }
    }

    fn put_collapsed(&mut self, character: char) {
        if character.is_whitespace() {
            self.collapsing = true;
            return;
        }
        if std::mem::take(&mut self.collapsing) {
            self.put(' ');
        }
        self.put(character);
    }

    fn put_trimmed(&mut self, character: char) {
        if !character.is_whitespace() {
            if let Some(mut pending) = self.pending_space.take() {
                update(&mut pending, character);
                self.hasher = pending;
                self.nonblank += character.len_utf8();
            } else {
                self.put(character);
            }
            return;
        }
        self.push_trimmed(character);
    }

    fn push_trimmed(&mut self, character: char) {
        if self.pending_space.is_none() {
            self.pending_space = Some(self.hasher.clone());
        }
        if let Some(pending) = &mut self.pending_space {
            update(pending, character);
        }
    }

    fn put(&mut self, character: char) {
        update(&mut self.hasher, character);
        if !character.is_whitespace() {
            self.nonblank += character.len_utf8();
        }
    }

    pub(super) fn finish(mut self, terminated: bool) -> (u64, usize) {
        let ending = if terminated && self.last == Some('\r') {
            self.last = None;
            LineEnding::CrLf
        } else if terminated {
            LineEnding::Lf
        } else {
            LineEnding::None
        };
        if let Some(character) = self.last.take() {
            self.emit(character);
        }
        if !self.ignore_line_endings {
            self.hasher.update(ending.as_str().as_bytes());
        }
        (self.hasher.digest(), self.nonblank)
    }
}

fn update(hasher: &mut Xxh3, character: char) {
    let mut buffer = [0_u8; 4];
    hasher.update(character.encode_utf8(&mut buffer).as_bytes());
}

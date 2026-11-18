//! The patterns a chunk end may cut in two: comment and string openers, operators and special
//! keywords. Part of the rule lexer; the hold-back in `chunking` looks them up by index.

use std::collections::HashSet;

use super::scan::{is_word, starts_with};
use crate::MAX_OPENERS;

/// One pattern; `folded` when it matches without regard to ASCII case (a special keyword of a
/// case-ignoring language).
#[derive(Debug, Clone)]
struct Hold {
    text: String,
    folded: bool,
}

/// Every pattern longer than one byte that a chunk end can cut, in the order the chunk state
/// numbers them: comment and string openers first, then operators, then special keywords.
#[derive(Debug, Clone, Default)]
pub(super) struct Holds {
    list: Vec<Hold>,
    longest: usize,
}

impl Holds {
    /// Collects the patterns. Each is at most `MAX_PATTERN_BYTES` long (the lexer drops longer
    /// ones first); a repeat is kept once. The chunk state numbers patterns with 16 bits, so
    /// the list stops at [`MAX_OPENERS`] entries and later patterns are not held back.
    pub(super) fn new(
        openers: &[&String],
        operators: &[&String],
        specials: &[&String],
        folded: bool,
    ) -> Self {
        let groups = [(openers, false), (operators, false), (specials, folded)];
        let mut seen = HashSet::new();
        let entries = groups.into_iter().flat_map(|(patterns, folded)| {
            (patterns.iter())
                .filter(|text| text.len() > 1)
                .map(move |text| (text, folded))
        });
        let list = entries
            .filter(|(text, folded)| seen.insert((text.as_str(), *folded)))
            .take(MAX_OPENERS)
            .map(|(text, folded)| Hold {
                text: (*text).clone(),
                folded,
            })
            .collect::<Vec<_>>();
        let longest = list.iter().map(|hold| hold.text.len()).max().unwrap_or(0);
        Self { list, longest }
    }

    /// The byte length of the longest held pattern; zero when there is none.
    pub(super) fn longest(&self) -> usize {
        self.longest
    }

    /// The pattern the chunk state numbers `index`.
    pub(super) fn text(&self, index: u16) -> Option<&String> {
        self.list.get(usize::from(index)).map(|hold| &hold.text)
    }

    /// The first pattern that `tail` only starts: `tail` is shorter than it and a prefix of it.
    /// A pattern that starts with a word char needs `behind` false (no word char precedes it).
    pub(super) fn find(&self, tail: &str, behind: bool) -> Option<u16> {
        let found = self.list.iter().position(|hold| {
            hold.text.len() > tail.len()
                && starts_with(&hold.text, tail, hold.folded)
                && (!hold.text.starts_with(is_word) || !behind)
        })?;
        u16::try_from(found).ok()
    }
}

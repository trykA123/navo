use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::paging::Needle;

pub const MARK_COLOURS: u8 = 8;
pub const MAX_MARKS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarkError {
    #[error("mark colour {0} is not one of the {MARK_COLOURS} colours")]
    Colour(u8),
    #[error("a file holds at most {MAX_MARKS} marks")]
    Full,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub needle: Needle,
    pub colour: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkSet {
    marks: Vec<Mark>,
}

impl MarkSet {
    pub fn set(&mut self, _needle: Needle, _colour: u8) -> Result<(), MarkError> {
        Ok(())
    }

    pub fn remove(&mut self, _needle: &Needle) -> bool {
        false
    }

    pub fn clear(&mut self) {}

    pub fn iter(&self) -> impl Iterator<Item = &Mark> {
        self.marks.iter().take(0)
    }

    #[must_use]
    pub fn colour_of(&self, _needle: &Needle) -> Option<u8> {
        None
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmarks {
    starts: BTreeSet<u64>,
}

impl Bookmarks {
    pub fn toggle(&mut self, _at: u64) -> bool {
        false
    }

    #[must_use]
    pub fn next_after(&self, _at: u64) -> Option<u64> {
        None
    }

    #[must_use]
    pub fn previous_before(&self, _at: u64) -> Option<u64> {
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.starts.iter().copied().take(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchHistory {
    newest_first: Vec<Needle>,
}

impl SearchHistory {
    pub const MAX: usize = 50;

    pub fn push(&mut self, _needle: Needle) {}

    pub fn iter(&self) -> impl Iterator<Item = &Needle> {
        self.newest_first.iter().take(0)
    }
}

#[must_use]
pub fn ticks(_spans: &[(u64, u8)], _len: u64, _rows: u32) -> Vec<(u32, u8)> {
    Vec::new()
}

#[cfg(test)]
mod tests;

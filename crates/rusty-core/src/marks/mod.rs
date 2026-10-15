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
    pub fn set(&mut self, needle: Needle, colour: u8) -> Result<(), MarkError> {
        if colour >= MARK_COLOURS {
            return Err(MarkError::Colour(colour));
        }
        if let Some(mark) = self.marks.iter_mut().find(|mark| mark.needle == needle) {
            mark.colour = colour;
            return Ok(());
        }
        if self.marks.len() >= MAX_MARKS {
            return Err(MarkError::Full);
        }
        self.marks.push(Mark { needle, colour });
        Ok(())
    }

    pub fn remove(&mut self, needle: &Needle) -> bool {
        let before = self.marks.len();
        self.marks.retain(|mark| mark.needle != *needle);
        self.marks.len() != before
    }

    pub fn clear(&mut self) {
        self.marks.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Mark> {
        self.marks.iter()
    }

    #[must_use]
    pub fn colour_of(&self, needle: &Needle) -> Option<u8> {
        self.marks
            .iter()
            .find(|mark| mark.needle == *needle)
            .map(|mark| mark.colour)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmarks {
    starts: BTreeSet<u64>,
}

impl Bookmarks {
    pub fn toggle(&mut self, at: u64) -> bool {
        if self.starts.remove(&at) {
            return false;
        }
        self.starts.insert(at);
        true
    }

    #[must_use]
    pub fn next_after(&self, at: u64) -> Option<u64> {
        self.starts
            .range(at.saturating_add(1)..)
            .next()
            .filter(|_| at < u64::MAX)
            .or_else(|| self.starts.first())
            .copied()
    }

    #[must_use]
    pub fn previous_before(&self, at: u64) -> Option<u64> {
        self.starts
            .range(..at)
            .next_back()
            .or_else(|| self.starts.last())
            .copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.starts.iter().copied()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchHistory {
    newest_first: Vec<Needle>,
}

impl SearchHistory {
    pub const MAX: usize = 50;

    pub fn push(&mut self, needle: Needle) {
        if needle.is_empty() {
            return;
        }
        self.newest_first.retain(|old| *old != needle);
        self.newest_first.insert(0, needle);
        self.newest_first.truncate(Self::MAX);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Needle> {
        self.newest_first.iter()
    }
}

#[must_use]
pub fn ticks(spans: &[(u64, u8)], len: u64, rows: u32) -> Vec<(u32, u8)> {
    if len == 0 || rows == 0 {
        return Vec::new();
    }
    let last = rows - 1;
    let mut ticks: Vec<(u32, u8)> = spans
        .iter()
        .map(|&(offset, colour)| {
            let row = u128::from(offset) * u128::from(rows) / u128::from(len);
            (u32::try_from(row).unwrap_or(last).min(last), colour)
        })
        .collect();
    ticks.sort_unstable();
    ticks.dedup();
    ticks
}

#[cfg(test)]
mod tests;

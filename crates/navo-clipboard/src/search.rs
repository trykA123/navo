//! In-memory search over history and saved rows.

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::model::{Content, Saved, SavedKind, Scope};
use crate::scope::same_folder;
use crate::store::Stored;

/// The most highlight ranges one row reports.
const MAX_RANGES: usize = 64;

/// The palette filter that picks the rows to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    All,
    Recent,
    Saved,
    Secrets,
    Notes,
}

/// The text a highlight range points into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Body,
    /// Ranges index into `join_paths` of the row's paths.
    Paths,
    User,
}

/// A matching history or saved row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Item<'a> {
    History(&'a Stored),
    Saved(&'a Saved),
}

/// The rows a search reads.
#[derive(Clone, Copy)]
pub struct Rows<'a> {
    pub history: &'a [Stored],
    pub saved: &'a [Saved],
}

/// One matching row and where it matched.
#[derive(Clone, PartialEq, Eq)]
pub struct Found<'a> {
    pub item: Item<'a>,
    pub field: Option<Field>,
    pub ranges: Vec<Range<usize>>,
}

impl fmt::Debug for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::History(stored) => f
                .debug_struct("History")
                .field("id", &stored.id)
                .field("content", &stored.entry.content)
                .finish(),
            Self::Saved(saved) => f
                .debug_struct("Saved")
                .field("id", &saved.id)
                .field("kind", &saved.kind)
                .field("title_len", &saved.title.len())
                .field("body_len", &saved.body.as_ref().map(String::len))
                .field("user_len", &saved.user.as_ref().map(String::len))
                .finish(),
        }
    }
}

impl fmt::Debug for Found<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lengths: Vec<usize> = self
            .ranges
            .iter()
            .map(|range| range.end.saturating_sub(range.start))
            .collect();
        f.debug_struct("Found")
            .field("item", &self.item)
            .field("field", &self.field)
            .field("range_lengths", &lengths)
            .finish()
    }
}

/// Searches the rows of one mode. An empty query keeps every row in the mode.
pub fn search<'a>(
    rows: Rows<'a>,
    mode: Mode,
    query: &str,
    folder: Option<&Path>,
) -> Vec<Found<'a>> {
    let needle = fold(query);
    match mode {
        Mode::All => all(rows, &needle),
        Mode::Recent => history(rows.history, &needle),
        Mode::Saved | Mode::Secrets | Mode::Notes => saved(rows.saved, mode, &needle, folder),
    }
}

fn all<'a>(rows: Rows<'a>, needle: &str) -> Vec<Found<'a>> {
    let mut found = Vec::with_capacity(rows.history.len() + rows.saved.len());
    found.extend(
        rows.history
            .iter()
            .filter_map(|stored| history_found(stored, needle)),
    );
    found.extend(
        rows.saved
            .iter()
            .filter_map(|saved| saved_found(saved, needle)),
    );
    found
}

fn history<'a>(rows: &'a [Stored], needle: &str) -> Vec<Found<'a>> {
    rows.iter()
        .filter_map(|stored| history_found(stored, needle))
        .collect()
}

fn saved<'a>(rows: &'a [Saved], mode: Mode, needle: &str, folder: Option<&Path>) -> Vec<Found<'a>> {
    if mode == Mode::Notes {
        return notes(rows, needle, folder);
    }
    rows.iter()
        .filter(|item| mode_includes(mode, item.kind))
        .filter_map(|item| saved_found(item, needle))
        .collect()
}

fn notes<'a>(rows: &'a [Saved], needle: &str, folder: Option<&Path>) -> Vec<Found<'a>> {
    let mut here = Vec::new();
    let mut other = Vec::new();
    for item in rows.iter().filter(|item| item.kind == SavedKind::Note) {
        let Some(found) = saved_found(item, needle) else {
            continue;
        };
        if is_here(item, folder) {
            here.push(found);
        } else {
            other.push(found);
        }
    }
    here.extend(other);
    here
}

fn mode_includes(mode: Mode, kind: SavedKind) -> bool {
    match mode {
        Mode::Saved => !matches!(kind, SavedKind::Secret | SavedKind::Note),
        Mode::Secrets => kind == SavedKind::Secret,
        Mode::All | Mode::Recent | Mode::Notes => true,
    }
}

fn is_here(item: &Saved, folder: Option<&Path>) -> bool {
    let (Scope::Folder(scope), Some(folder)) = (&item.scope, folder) else {
        return false;
    };
    same_folder(scope, folder)
}

fn history_found<'a>(stored: &'a Stored, needle: &str) -> Option<Found<'a>> {
    let item = Item::History(stored);
    if needle.is_empty() {
        return Some(Found::plain(item));
    }
    match &stored.entry.content {
        Content::Text(text) => matched(item, Field::Body, text, needle),
        Content::Paths { paths, .. } => {
            let joined = join_paths(paths);
            matched(item, Field::Paths, &joined, needle)
        }
    }
}

/// Joins lossy path strings with `\n`, the text `Field::Paths` ranges index.
pub fn join_paths(paths: &[PathBuf]) -> String {
    let paths: Vec<_> = paths.iter().map(|path| path.to_string_lossy()).collect();
    paths.join("\n")
}

fn saved_found<'a>(saved: &'a Saved, needle: &str) -> Option<Found<'a>> {
    let item = Item::Saved(saved);
    if needle.is_empty() {
        return Some(Found::plain(item));
    }
    if let Some(found) = matched(item, Field::Title, &saved.title, needle) {
        return Some(found);
    }
    if saved.kind == SavedKind::Secret {
        return saved
            .user
            .as_deref()
            .and_then(|user| matched(item, Field::User, user, needle));
    }
    saved
        .body
        .as_deref()
        .and_then(|body| matched(item, Field::Body, body, needle))
}

fn matched<'a>(item: Item<'a>, field: Field, text: &str, needle: &str) -> Option<Found<'a>> {
    let ranges = find_ranges(text, needle);
    if ranges.is_empty() {
        return None;
    }
    Some(Found {
        item,
        field: Some(field),
        ranges,
    })
}

impl<'a> Found<'a> {
    fn plain(item: Item<'a>) -> Self {
        Self {
            item,
            field: None,
            ranges: Vec::new(),
        }
    }
}

pub(crate) fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

struct Folded {
    text: String,
    starts: Vec<u32>,
    ends: Vec<u32>,
}

impl Folded {
    fn new(original: &str) -> Option<Self> {
        let mut text = String::with_capacity(original.len());
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for (offset, character) in original.char_indices() {
            let start = u32::try_from(offset).ok()?;
            let end = u32::try_from(offset + character.len_utf8()).ok()?;
            for lower in character.to_lowercase() {
                text.push(lower);
                starts.resize(text.len(), start);
                ends.resize(text.len(), end);
            }
        }
        Some(Self { text, starts, ends })
    }

    fn original(&self, start: usize, end: usize) -> Option<Range<usize>> {
        let first = *self.starts.get(start)?;
        let last = *self.ends.get(end - 1)?;
        Some(usize::try_from(first).ok()?..usize::try_from(last).ok()?)
    }
}

pub(crate) fn find_ranges(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() || text.is_empty() {
        return Vec::new();
    }
    if text.is_ascii() {
        return match_ranges(&text.to_ascii_lowercase(), needle, |start, end| {
            Some(start..end)
        });
    }
    let folded = fold(text);
    if !folded.contains(needle) {
        return Vec::new();
    }
    let Some(map) = Folded::new(text) else {
        return Vec::new();
    };
    match_ranges(&map.text, needle, |start, end| map.original(start, end))
}

fn match_ranges(
    haystack: &str,
    needle: &str,
    map: impl Fn(usize, usize) -> Option<Range<usize>>,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while ranges.len() < MAX_RANGES {
        let Some(at) = haystack[from..].find(needle) else {
            break;
        };
        let start = from + at;
        let end = start + needle.len();
        let Some(range) = map(start, end) else {
            break;
        };
        ranges.push(range);
        from = end;
    }
    ranges
}

#[cfg(test)]
mod tests;

use std::ops::Range;

use rusty_core::Location;

pub const LONG_PATH: usize = 260;

#[must_use]
pub fn changed_ranges(old: &str, new: &str) -> Vec<Range<usize>> {
    let old: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let prefix = old
        .iter()
        .zip(&new_chars)
        .take_while(|(a, b)| a == b)
        .count();
    let room = old.len().min(new_chars.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new_chars.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let end = new_chars.len() - suffix;
    if prefix >= end {
        return Vec::new();
    }
    let byte = |chars: usize| {
        new.char_indices()
            .nth(chars)
            .map_or(new.len(), |(at, _)| at)
    };
    std::iter::once(byte(prefix)..byte(end)).collect()
}

#[must_use]
pub fn is_long_path(parent: &Location, name: &str) -> bool {
    let folder = parent.as_str();
    let joint = usize::from(!folder.ends_with(['/', '\\']));
    folder.chars().count() + joint + name.chars().count() > LONG_PATH
}

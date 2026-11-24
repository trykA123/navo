//! Line operations for the editor: sort, dedup, join, and trim trailing blanks.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::transforms::split_lines;

/// How `sort_lines` orders lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKind {
    /// Plain byte order.
    Alpha,
    /// Numeric runs compare by value.
    Natural,
    /// Shorter lines first.
    Length,
    /// Reverse the input order.
    Reverse,
}

/// Options for one sort: kind, direction, and case folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortSpec {
    pub kind: SortKind,
    pub descending: bool,
    pub ignore_case: bool,
}

/// Sorts the lines of `text`; returns them joined with the same ending.
pub fn sort_lines(text: &str, spec: SortSpec) -> String {
    let parsed = split_lines(text);
    let mut lines = parsed.lines.clone();
    if spec.kind == SortKind::Reverse {
        lines.reverse();
    } else {
        lines.sort_by(|a, b| {
            let order = compare_lines(a, b, spec);
            if spec.descending {
                order.reverse()
            } else {
                order
            }
        });
    }
    parsed.join(&lines)
}

/// Compares two lines for `sort_lines` before the descending flip.
fn compare_lines(a: &str, b: &str, spec: SortSpec) -> Ordering {
    match spec.kind {
        SortKind::Alpha if spec.ignore_case => a.to_lowercase().cmp(&b.to_lowercase()),
        SortKind::Alpha => a.cmp(b),
        SortKind::Natural => {
            let (a_lower, b_lower) = (a.to_lowercase(), b.to_lowercase());
            crate::natural::natural_cmp(&a_lower, &b_lower).then_with(|| a.cmp(b))
        }
        SortKind::Length => a.chars().count().cmp(&b.chars().count()),
        SortKind::Reverse => Ordering::Equal,
    }
}

/// Keeps the first occurrence of each exact line, in input order.
pub fn dedup_lines(text: &str) -> String {
    let parsed = split_lines(text);
    let mut seen = HashSet::new();
    let lines: Vec<&str> = parsed
        .lines
        .iter()
        .copied()
        .filter(|line| seen.insert(*line))
        .collect();
    parsed.join(&lines)
}

/// Joins the lines of `text` with `separator` and never adds a final ending.
pub fn join_lines(text: &str, separator: &str) -> String {
    split_lines(text).lines.join(separator)
}

/// Removes trailing spaces and tabs from each line, keeping the line endings.
pub fn trim_trailing(text: &str) -> String {
    let parsed = split_lines(text);
    let lines: Vec<&str> = parsed
        .lines
        .iter()
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect();
    parsed.join(&lines)
}

//! Find in Settings: which rows a search admits, and the lines a page or a search shows.

use std::ops::Range;

use super::fields::{FIELDS, Field, FieldSpec, Section};
use super::fold::{Folds, fold_section, page_rows};
use super::help_page::WORDS;
use super::index_page::PROGRESS_WORDS;
use super::pages::{group_of, ordered_rows};
use super::search_page::{HISTORY_WORDS, SERVICE_WORDS};
use super::steps::{DIAGNOSTICS_ROW, HELP_ROW, rows_in};
use super::values::Values;

const DIAGNOSTICS_WORDS: &str = "diagnostics cache storage report";
const _: () = assert!(
    FIELDS.len() <= u128::BITS as usize,
    "one changed bit per field"
);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    words: Vec<String>,
    hidden: Option<Field>,
    folds: Folds,
    changed_only: bool,
    changed: u128,
}

impl Filter {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            words: text.split_whitespace().map(str::to_lowercase).collect(),
            hidden: None,
            folds: Folds::default(),
            changed_only: false,
            changed: 0,
        }
    }

    /// Shows the advanced settings of the open folds in `folds` while the words are empty.
    #[must_use]
    pub fn folding(mut self, folds: Folds) -> Self {
        self.folds = folds;
        self
    }

    #[must_use]
    pub fn folds(&self) -> Folds {
        self.folds
    }

    /// Hides the row of `field` whatever the words say; `None` hides nothing.
    #[must_use]
    pub fn hiding(mut self, field: Option<Field>) -> Self {
        self.hidden = field;
        self
    }

    /// Restricts admitted rows to fields whose current value differs from the default.
    #[must_use]
    pub fn changed_only(mut self, changed_only: bool) -> Self {
        self.changed_only = changed_only;
        self
    }

    /// True when the changed-only filter is active.
    #[must_use]
    pub fn is_changed_only(&self) -> bool {
        self.changed_only
    }

    /// Records which fields are currently changed from `values` for `changed_only` filtering.
    #[must_use]
    pub fn marking(mut self, values: &Values) -> Self {
        self.changed = FIELDS
            .iter()
            .enumerate()
            .filter(|(_, spec)| values.is_changed(spec.field))
            .fold(0u128, |mask, (index, _)| mask | (1u128 << index));
        self
    }

    /// True when the Find query has at least one search word.
    #[must_use]
    pub fn has_words(&self) -> bool {
        !self.words.is_empty()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && !self.changed_only
    }

    fn is_field_changed(&self, field: Field) -> bool {
        self.changed & (1u128 << field.index()) != 0
    }

    /// True when the row of `spec` is shown: Find admits it and, with no words, its fold
    /// (if any) is open.
    #[must_use]
    pub fn admits(&self, spec: &FieldSpec) -> bool {
        self.matches(spec) && (!self.is_empty() || self.folds.shows(spec.field))
    }

    /// True when Find admits `spec`, whatever the fold state.
    pub(super) fn matches(&self, spec: &FieldSpec) -> bool {
        if spec.section == Section::About {
            return false;
        }
        if !spec.field.is_shown()
            || (spec.field.is_windows_only() && !cfg!(windows))
            || self.hidden == Some(spec.field)
            || (spec.table() == "compare" && !crate::compare_visible::compare_visible())
            || (self.changed_only && !self.is_field_changed(spec.field))
        {
            return false;
        }
        if self.words.is_empty() {
            return true;
        }
        let text = format!("{} {} {}", spec.label(), spec.key, spec.help()).to_lowercase();
        self.words.iter().all(|word| text.contains(word))
    }

    #[must_use]
    pub fn admits_text(&self, text: &str) -> bool {
        !self.changed_only && self.words.iter().all(|word| text.contains(word.as_str()))
    }

    /// Byte ranges of every search word found in `text` (case-folded when byte lengths match),
    /// sorted and merged when overlapping or adjacent.
    #[must_use]
    pub fn hits(&self, text: &str) -> Vec<Range<usize>> {
        if self.words.is_empty() {
            return Vec::new();
        }
        let lower = text.to_lowercase();
        if lower.len() != text.len() {
            return Vec::new();
        }
        let mut ranges: Vec<Range<usize>> = self
            .words
            .iter()
            .filter(|word| !word.is_empty())
            .flat_map(|word| {
                lower
                    .match_indices(word.as_str())
                    .filter_map(|(start, found)| {
                        let end = start + found.len();
                        (text.is_char_boundary(start) && text.is_char_boundary(end))
                            .then_some(start..end)
                    })
            })
            .collect();
        ranges.sort_unstable_by_key(|range| (range.start, range.end));
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        merged
    }

    /// Number of admitted setting rows and extra blocks on `section` for the current filter.
    #[must_use]
    pub fn count_in(&self, section: Section) -> usize {
        let rows = page_rows(self, section)
            .into_iter()
            .filter(|index| FIELDS.get(*index).is_some())
            .count();
        let extras = match section {
            Section::About => {
                usize::from(self.admits_text(WORDS))
                    + usize::from(self.admits_text(DIAGNOSTICS_WORDS))
            }
            Section::Search => extra_lines(self, section).len(),
            _ => 0,
        };
        rows + extras
    }

    #[must_use]
    pub fn first_row(&self) -> Option<usize> {
        ordered_rows()
            .find(|(_, spec)| self.admits(spec))
            .map(|(index, _)| index)
    }

    #[must_use]
    pub fn first_in(&self, section: Section) -> Option<usize> {
        if section == Section::About {
            return self.admits_text(WORDS).then_some(HELP_ROW).or_else(|| {
                self.admits_text(DIAGNOSTICS_WORDS)
                    .then_some(DIAGNOSTICS_ROW)
            });
        }
        rows_in(section)
            .find(|(_, spec)| self.admits(spec))
            .map(|(index, _)| index)
    }

    /// The row to select after the filter changes: `row` while it stays shown (a fold row
    /// with Find empty, or an admitted setting), else the first admitted row, else `row`.
    #[must_use]
    pub fn nearest(&self, row: usize) -> usize {
        if self.is_empty() && fold_section(row).is_some() {
            return row;
        }
        match FIELDS.get(row) {
            Some(spec) if self.admits(spec) => row,
            _ => self.first_row().unwrap_or(row),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    Section(Section),
    Group(&'static str),
    Row(usize),
    Fold(usize),
    Progress,
    Service,
    History,
}

#[must_use]
pub fn lines(filter: &Filter, current: Section) -> Vec<Line> {
    if filter.is_empty() {
        return section_lines(filter, current);
    }
    Section::ALL
        .into_iter()
        .flat_map(|section| section_lines(filter, section))
        .collect()
}

fn section_lines(filter: &Filter, section: Section) -> Vec<Line> {
    let rows = page_rows(filter, section);
    let extras = extra_lines(filter, section);
    if rows.is_empty() && extras.is_empty() {
        return Vec::new();
    }
    let mut group = None;
    let mut found = vec![Line::Section(section)];
    for index in rows {
        let Some(spec) = FIELDS.get(index) else {
            found.push(Line::Fold(index));
            continue;
        };
        if group_of(spec.field) != group {
            group = group_of(spec.field);
            found.extend(group.map(Line::Group));
        }
        found.push(Line::Row(index));
    }
    found.extend(extras);
    found
}

fn extra_lines(filter: &Filter, section: Section) -> Vec<Line> {
    if section != Section::Search {
        return Vec::new();
    }
    [
        (Line::Progress, PROGRESS_WORDS),
        (Line::Service, SERVICE_WORDS),
        (Line::History, HISTORY_WORDS),
    ]
    .into_iter()
    .filter(|(_, words)| filter.admits_text(words))
    .map(|(line, _)| line)
    .collect()
}

//! Find in Settings: which rows a search admits, and the lines a page or a search shows.

use super::fields::{FIELDS, Field, FieldSpec, Section};
use super::fold::{Folds, page_rows};
use super::help_page::WORDS;
use super::index_page::PROGRESS_WORDS;
use super::pages::{group_of, ordered_rows};
use super::search_page::{HISTORY_WORDS, SERVICE_WORDS};
use super::steps::{DIAGNOSTICS_ROW, HELP_ROW, rows_in};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    words: Vec<String>,
    hidden: Option<Field>,
    folds: Folds,
}

impl Filter {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            words: text.split_whitespace().map(str::to_lowercase).collect(),
            hidden: None,
            folds: Folds::default(),
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
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
        {
            return false;
        }
        if self.is_empty() {
            return true;
        }
        let text = format!("{} {} {}", spec.label(), spec.key, spec.help()).to_lowercase();
        self.words.iter().all(|word| text.contains(word))
    }

    #[must_use]
    pub fn admits_text(&self, text: &str) -> bool {
        self.words.iter().all(|word| text.contains(word.as_str()))
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
                self.admits_text("diagnostics cache storage report")
                    .then_some(DIAGNOSTICS_ROW)
            });
        }
        rows_in(section)
            .find(|(_, spec)| self.admits(spec))
            .map(|(index, _)| index)
    }

    #[must_use]
    pub fn nearest(&self, row: usize) -> usize {
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

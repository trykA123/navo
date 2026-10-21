use super::fields::{FIELDS, FieldSpec, Section};
use super::search_page::{HISTORY_WORDS, SERVICE_WORDS};
use super::steps::rows_in;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter(Vec<String>);

impl Filter {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self(text.split_whitespace().map(str::to_lowercase).collect())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn admits(&self, spec: &FieldSpec) -> bool {
        if spec.field.is_windows_only() && !cfg!(windows) {
            return false;
        }
        if self.is_empty() {
            return true;
        }
        let text = format!("{} {} {}", spec.label(), spec.key, spec.help()).to_lowercase();
        self.0.iter().all(|word| text.contains(word))
    }

    #[must_use]
    pub fn admits_text(&self, text: &str) -> bool {
        self.0.iter().all(|word| text.contains(word.as_str()))
    }

    #[must_use]
    pub fn first_row(&self) -> Option<usize> {
        FIELDS.iter().position(|spec| self.admits(spec))
    }

    #[must_use]
    pub fn first_in(&self, section: Section) -> Option<usize> {
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
    let rows: Vec<_> = rows_in(section)
        .filter(|(_, spec)| filter.admits(spec))
        .collect();
    let extras = extra_lines(filter, section);
    if rows.is_empty() && extras.is_empty() {
        return Vec::new();
    }
    let mut group = None;
    let mut found = vec![Line::Section(section)];
    for (index, spec) in rows {
        if section == Section::Display && spec.group() != group {
            group = spec.group();
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
        (Line::Service, SERVICE_WORDS),
        (Line::History, HISTORY_WORDS),
    ]
    .into_iter()
    .filter(|(_, words)| filter.admits_text(words))
    .map(|(line, _)| line)
    .collect()
}

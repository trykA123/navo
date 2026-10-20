use std::collections::{HashMap, HashSet};

use crate::{Location, NameError, check_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Insensitive,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub parent: Location,
    pub old: String,
    pub new: String,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingName {
    pub parent: Location,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Existing {
    pub(super) names: Vec<ExistingName>,
}

impl Existing {
    pub fn new(names: Vec<ExistingName>) -> Self {
        Self { names }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Invalid(NameError),
    Empty,
    Duplicate(usize),
    Exists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Changes { case_only: bool },
    Unchanged,
    Excluded,
    Problem(Problem),
}

pub(super) type Key = (Location, String);

pub fn fold_key(name: &str, fold: Fold) -> String {
    match fold {
        Fold::Exact => name.to_owned(),
        Fold::Insensitive => name.chars().map(fold_char).collect(),
    }
}

fn fold_char(ch: char) -> char {
    match single(ch.to_uppercase()) {
        Some(upper) => single(upper.to_lowercase()).unwrap_or(upper),
        None => single(ch.to_lowercase()).unwrap_or(ch),
    }
}

fn single(mut chars: impl Iterator<Item = char>) -> Option<char> {
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

pub(super) fn key(parent: &Location, name: &str, fold: Fold) -> Key {
    (parent.clone(), fold_key(name, fold))
}

fn is_active(row: &Row) -> bool {
    !row.excluded && row.new != row.old
}

fn name_problem(name: &str) -> Option<Problem> {
    if name.trim().is_empty() {
        return Some(Problem::Empty);
    }
    check_name(name).err().map(Problem::Invalid)
}

fn new_name_groups(rows: &[Row], fold: Fold) -> HashMap<Key, Vec<usize>> {
    let mut groups: HashMap<Key, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate().filter(|(_, row)| !row.excluded) {
        if row.new == row.old || name_problem(&row.new).is_none() {
            groups
                .entry(key(&row.parent, &row.new, fold))
                .or_default()
                .push(index);
        }
    }
    groups
}

fn occupied_names(rows: &[Row], existing: &Existing, fold: Fold) -> HashSet<Key> {
    let vacated: HashSet<Key> = rows
        .iter()
        .filter(|row| is_active(row))
        .map(|row| key(&row.parent, &row.old, fold))
        .collect();
    let listed = existing
        .names
        .iter()
        .map(|item| key(&item.parent, &item.name, fold));
    let excluded = rows
        .iter()
        .filter(|row| row.excluded)
        .map(|row| key(&row.parent, &row.old, fold));
    listed
        .filter(|name| !vacated.contains(name))
        .chain(excluded)
        .collect()
}

struct Lookup {
    groups: HashMap<Key, Vec<usize>>,
    occupied: HashSet<Key>,
    fold: Fold,
}

pub fn check_rows(rows: &[Row], existing: &Existing, fold: Fold) -> Vec<RowState> {
    let lookup = Lookup {
        groups: new_name_groups(rows, fold),
        occupied: occupied_names(rows, existing, fold),
        fold,
    };
    rows.iter()
        .enumerate()
        .map(|(index, row)| row_state(index, row, &lookup))
        .collect()
}

fn row_state(index: usize, row: &Row, lookup: &Lookup) -> RowState {
    if row.excluded {
        return RowState::Excluded;
    }
    if row.new == row.old {
        return RowState::Unchanged;
    }
    if let Some(problem) = name_problem(&row.new) {
        return RowState::Problem(problem);
    }
    let fold = lookup.fold;
    let target = key(&row.parent, &row.new, fold);
    let twin = lookup
        .groups
        .get(&target)
        .and_then(|group| group.iter().copied().find(|&other| other != index));
    if let Some(other) = twin {
        return RowState::Problem(Problem::Duplicate(other));
    }
    if lookup.occupied.contains(&target) {
        return RowState::Problem(Problem::Exists);
    }
    RowState::Changes {
        case_only: fold == Fold::Insensitive && target.1 == fold_key(&row.old, fold),
    }
}

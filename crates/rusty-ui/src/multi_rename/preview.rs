use std::collections::HashMap;

use rusty_core::OperationPlan;
use rusty_core::rename::check::{Existing, Fold, Problem, Row, RowState, fold_key};
use rusty_core::rename::{Compiled, Subject, plan_batch};

use super::Form;
use super::item::Item;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    pub new: String,
    pub state: RowState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub rename: usize,
    pub unchanged: usize,
    pub excluded: usize,
    pub problems: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub generation: u64,
    pub rows: Vec<PreviewRow>,
    pub summary: Summary,
    pub plan: Option<OperationPlan>,
}

#[derive(Clone, Copy)]
pub struct Listing<'a> {
    pub existing: &'a Existing,
    pub fold: Fold,
}

impl Preview {
    #[must_use]
    pub fn compute(
        compiled: &Compiled,
        items: &[Item],
        listing: Listing<'_>,
        generation: u64,
    ) -> Self {
        Self::from_rows(
            rows(compiled, items, None, listing.fold),
            listing,
            generation,
        )
    }

    #[must_use]
    pub fn compute_form(
        form: &Form,
        items: &[Item],
        listing: Listing<'_>,
        generation: u64,
    ) -> Option<Self> {
        if form.names_error(items.len()).is_some() {
            return None;
        }
        let compiled = form.compile().ok()?;
        Some(Self::from_rows(
            rows(&compiled, items, Some(form), listing.fold),
            listing,
            generation,
        ))
    }

    fn from_rows(rows: Vec<Row>, listing: Listing<'_>, generation: u64) -> Self {
        let seed = u32::try_from(generation & u64::from(u32::MAX)).unwrap_or_default();
        let batch = plan_batch(&rows, listing.existing, listing.fold, seed);
        let summary = summarize(&batch.states);
        let rows = rows
            .into_iter()
            .zip(batch.states)
            .map(|(row, state)| PreviewRow {
                new: row.new,
                state,
            })
            .collect();
        Self {
            generation,
            rows,
            summary,
            plan: batch.plan,
        }
    }

    #[must_use]
    pub fn can_run(&self) -> bool {
        self.summary.problems == 0 && self.plan.is_some()
    }

    #[must_use]
    pub fn visible(&self, problems_only: bool) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&index| !problems_only || is_problem(&self.rows[index]))
            .collect()
    }

    #[must_use]
    pub fn next_problem(&self, from: Option<usize>) -> Option<usize> {
        let len = self.rows.len();
        let start = from.map_or(0, |row| row + 1);
        (0..len)
            .map(|offset| (start + offset) % len)
            .find(|&index| is_problem(&self.rows[index]))
    }

    #[must_use]
    pub fn previous_problem(&self, from: Option<usize>) -> Option<usize> {
        let len = self.rows.len();
        let start = from.unwrap_or(0);
        (1..=len)
            .map(|offset| (start + len - offset) % len)
            .find(|&index| is_problem(&self.rows[index]))
    }

    #[must_use]
    pub fn duplicate_of(&self, row: usize) -> Option<usize> {
        match self.rows.get(row)?.state {
            RowState::Problem(Problem::Duplicate(other)) => Some(other),
            _ => None,
        }
    }
}

fn is_problem(row: &PreviewRow) -> bool {
    matches!(row.state, RowState::Problem(_))
}

fn rows(compiled: &Compiled, items: &[Item], form: Option<&Form>, fold: Fold) -> Vec<Row> {
    let mut ordinal = 0u64;
    let mut folders = HashMap::<String, u64>::new();
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let excluded = form.map_or(item.excluded, |form| form.is_excluded(index));
            let new = if excluded {
                item.name.clone()
            } else {
                let counter = if form.is_some_and(|form| form.counter_per_folder) {
                    folders
                        .entry(fold_key(item.parent.as_str(), fold))
                        .or_default()
                } else {
                    &mut ordinal
                };
                let name = form.and_then(Form::edited_names).map_or_else(
                    || new_name(compiled, item, *counter),
                    |names| names[index].clone(),
                );
                *counter += 1;
                name
            };
            Row {
                parent: item.parent.clone(),
                old: item.name.clone(),
                new,
                excluded,
            }
        })
        .collect()
}

fn new_name(compiled: &Compiled, item: &Item, ordinal: u64) -> String {
    let lookup = |group: &str, field: &str| item.fields.get(group, field);
    let subject = Subject {
        fields: Some(&lookup),
        ..item.subject()
    };
    compiled.new_name(&subject, ordinal)
}

fn summarize(states: &[RowState]) -> Summary {
    let mut summary = Summary::default();
    for state in states {
        match state {
            RowState::Changes { .. } => summary.rename += 1,
            RowState::Unchanged => summary.unchanged += 1,
            RowState::Excluded => summary.excluded += 1,
            RowState::Problem(_) => summary.problems += 1,
        }
    }
    summary
}

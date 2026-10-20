use rusty_core::OperationPlan;
use rusty_core::rename::check::{Existing, Fold, Problem, Row, RowState};
use rusty_core::rename::{Compiled, plan_batch};

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
        let rows = rows(compiled, items);
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

fn rows(compiled: &Compiled, items: &[Item]) -> Vec<Row> {
    let mut ordinal = 0u64;
    items
        .iter()
        .map(|item| {
            let new = if item.excluded {
                item.name.clone()
            } else {
                let name = compiled.new_name(&item.subject(), ordinal);
                ordinal += 1;
                name
            };
            Row {
                parent: item.parent.clone(),
                old: item.name.clone(),
                new,
                excluded: item.excluded,
            }
        })
        .collect()
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

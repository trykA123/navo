use std::collections::HashMap;

use rusty_core::NameError;
use rusty_core::rename::check::{Fold, Problem, RowState, fold_key};
use rusty_l10n::t;

use crate::multi_rename::{Item, Preview};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Renames,
    CaseOnly,
    Swap,
    Unchanged,
    Excluded,
    Problem(Problem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub old: String,
    pub new: String,
    pub state: RowState,
    pub is_swap: bool,
}

impl Line {
    pub fn kind(&self) -> Kind {
        match self.state {
            RowState::Problem(problem) => Kind::Problem(problem),
            RowState::Excluded => Kind::Excluded,
            RowState::Unchanged => Kind::Unchanged,
            RowState::Changes { .. } if self.is_swap => Kind::Swap,
            RowState::Changes { case_only: true } => Kind::CaseOnly,
            RowState::Changes { case_only: false } => Kind::Renames,
        }
    }

    pub fn status(&self) -> String {
        match self.kind() {
            Kind::Renames => String::new(),
            Kind::CaseOnly => t!("rename-status-case-only"),
            Kind::Swap => t!("rename-status-swap"),
            Kind::Unchanged => t!("rename-status-unchanged"),
            Kind::Excluded => t!("rename-status-excluded"),
            Kind::Problem(problem) => problem_text(problem),
        }
    }
}

fn problem_text(problem: Problem) -> String {
    match problem {
        Problem::Exists => t!("rename-status-exists"),
        Problem::Empty | Problem::Invalid(NameError::Empty) => t!("rename-status-empty"),
        Problem::Duplicate(other) => t!("rename-status-same-name", row = other + 1),
        Problem::Invalid(NameError::Character(ch)) => {
            t!("rename-status-bad-character", character = ch.to_string())
        }
        Problem::Invalid(_) => t!("rename-status-reserved"),
    }
}

pub(super) fn lines(items: &[Item], preview: &Preview, fold: Fold) -> Vec<Line> {
    let mut lines: Vec<Line> = items
        .iter()
        .zip(&preview.rows)
        .map(|(item, row)| Line {
            old: item.name.clone(),
            new: row.new.clone(),
            state: row.state,
            is_swap: false,
        })
        .collect();
    mark_swaps(items, &mut lines, fold);
    lines
}

fn mark_swaps(items: &[Item], lines: &mut [Line], fold: Fold) {
    let renamed = |line: &Line| matches!(line.state, RowState::Changes { .. });
    let by_old: HashMap<_, _> = items
        .iter()
        .zip(lines.iter())
        .enumerate()
        .filter(|(_, (_, line))| renamed(line))
        .map(|(at, (item, line))| ((&item.parent, fold_key(&line.old, fold)), at))
        .collect();
    let swaps: Vec<usize> = (0..lines.len())
        .filter(|&at| renamed(&lines[at]))
        .filter(|&at| {
            let key = (&items[at].parent, fold_key(&lines[at].new, fold));
            by_old.get(&key).is_some_and(|&other| {
                other != at && fold_key(&lines[other].new, fold) == fold_key(&lines[at].old, fold)
            })
        })
        .collect();
    for at in swaps {
        lines[at].is_swap = true;
    }
}

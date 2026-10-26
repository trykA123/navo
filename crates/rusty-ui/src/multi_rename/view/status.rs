use std::collections::HashMap;
use std::ops::Range;

use rusty_core::Location;
use rusty_core::NameError;
use rusty_core::rename::Compiled;
use rusty_core::rename::check::{Fold, Problem, RowState, fold_key};
use rusty_l10n::t;

use crate::multi_rename::{Item, Preview, changed_ranges, is_long_path};

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
    pub parent: Location,
    pub old: String,
    pub new: String,
    pub state: RowState,
    pub is_swap: bool,
    pub is_long: bool,
    pub is_edited: bool,
    pub old_marks: Vec<Range<usize>>,
    pub new_marks: Vec<Range<usize>>,
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
        if let RowState::Problem(problem) = self.state {
            return problem_text(problem);
        }
        if self.is_long {
            return t!("rename-status-long-path");
        }
        if self.is_edited && self.state != RowState::Excluded {
            return t!("rename-status-edited");
        }
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

pub(super) fn lines(
    items: &[Item],
    preview: &Preview,
    fold: Fold,
    matcher: Option<&Compiled>,
) -> Vec<Line> {
    let mut lines: Vec<Line> = items
        .iter()
        .zip(&preview.rows)
        .map(|(item, row)| {
            let is_changed = matches!(row.state, RowState::Changes { .. });
            let is_excluded = row.state == RowState::Excluded;
            Line {
                parent: item.parent.clone(),
                old: item.name.clone(),
                new: row.new.clone(),
                state: row.state,
                is_swap: false,
                is_long: is_changed && is_long_path(&item.parent, &row.new),
                is_edited: false,
                old_marks: matcher
                    .filter(|_| !is_excluded)
                    .map(|compiled| compiled.match_ranges(&item.name))
                    .unwrap_or_default(),
                new_marks: if is_changed {
                    changed_ranges(&item.name, &row.new)
                } else {
                    Vec::new()
                },
            }
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

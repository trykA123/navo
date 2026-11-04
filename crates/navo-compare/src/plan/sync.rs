use std::collections::{HashMap, HashSet};

use navo_core::retention::MAX_NODES;
use navo_core::{OperationPlan, Totals};

use super::Sides;
use crate::model::{CancelToken, EntryKind, Generation, Reason, Row};
use crate::text::Direction;
use rules::decide;

mod count;
mod parts;
mod roots;
mod rules;
mod spelling;

/// Rows between two looks at the cancel token.
const CANCEL_EVERY: usize = 1_024;

pub use rules::{
    CantSync, Deletes, SAME_TIME_MS, SafetyNet, SyncAction, SyncMode, SyncRules, Verdict, recommend,
};

/// How the compare walk ended; only a `Final` compare plans a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Final,
    Walking,
    StoppedAtLimit,
    Cancelled,
    Failed(Reason),
}

#[derive(Debug, Clone, Copy)]
pub struct SyncRequest<'a> {
    pub rows: &'a [Row],
    pub generation: Generation,
    pub completion: Completion,
    pub sides: &'a Sides,
    pub rules: SyncRules,
    pub choices: &'a HashMap<u64, SyncAction>,
    /// The compare session's token; a cancelled build returns [`SyncBlock::Cancelled`].
    pub cancel: &'a CancelToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRefusal {
    pub row: u64,
    pub reason: CantSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSummary {
    pub to_right: Totals,
    pub to_left: Totals,
    pub deleted: Totals,
    pub replaces_newer: u64,
    pub same: u64,
    pub skipped: u64,
    pub cant_sync: Vec<RowRefusal>,
}

/// Run `copy` first; run `delete` only after it ends Done, only as a retained delete (Y1-D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub copy: Option<OperationPlan>,
    pub delete: Option<OperationPlan>,
    pub summary: SyncSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncBlock {
    NotFinal(Completion),
    StaleGeneration,
    DuplicateRow(u64),
    UnknownChoice(u64),
    Unresolved {
        conflicts: Vec<u64>,
        checking: Vec<u64>,
    },
    TooManySteps {
        steps: usize,
    },
    /// The two roots are one folder, or one lies inside the other.
    SameOrNestedRoots,
    /// A root has a `.` or `..` segment, so its text does not name one place.
    RootNotPlain,
    Cancelled,
}

/// Builds the sync plan for one finished compare generation.
///
/// # Errors
/// A [`SyncBlock`] when the compare, the rows or the choices cannot give one safe plan.
pub fn build(request: &SyncRequest<'_>) -> Result<SyncPlan, SyncBlock> {
    build_capped(request, MAX_NODES)
}

fn build_capped(request: &SyncRequest<'_>, cap: usize) -> Result<SyncPlan, SyncBlock> {
    stop_if_cancelled(request, 0)?;
    check_request(request)?;
    let fences = Fences::of(request)?;
    let sorted = sort_rows(request, &fences)?;
    let present = count::present_folders(request);
    let (candidates, mut summary) = parts::candidates(request, sorted, &present, cap)?;
    let candidates = parts::drop_collisions(candidates, request.sides, &mut summary);
    stop_if_cancelled(request, 0)?;
    let plan = parts::assemble(candidates, &present, summary);
    let largest = [&plan.copy, &plan.delete]
        .into_iter()
        .flatten()
        .map(|part| part.steps.len())
        .max()
        .unwrap_or(0);
    if largest > cap {
        return Err(SyncBlock::TooManySteps { steps: largest });
    }
    Ok(plan)
}

/// Checks the cancel token once every [`CANCEL_EVERY`] rows.
fn stop_if_cancelled(request: &SyncRequest<'_>, index: usize) -> Result<(), SyncBlock> {
    if index.is_multiple_of(CANCEL_EVERY) && request.cancel.is_cancelled() {
        Err(SyncBlock::Cancelled)
    } else {
        Ok(())
    }
}

fn check_request(request: &SyncRequest<'_>) -> Result<(), SyncBlock> {
    if request.completion != Completion::Final {
        return Err(SyncBlock::NotFinal(request.completion));
    }
    roots::check(request.sides)?;
    if request
        .rows
        .iter()
        .any(|row| row.generation != request.generation)
    {
        return Err(SyncBlock::StaleGeneration);
    }
    let mut ids = HashSet::with_capacity(request.rows.len());
    if let Some(row) = request.rows.iter().find(|row| !ids.insert(row.id)) {
        return Err(SyncBlock::DuplicateRow(row.id));
    }
    // The smallest unknown id keeps the answer independent of map order.
    match request.choices.keys().filter(|id| !ids.contains(*id)).min() {
        Some(id) => Err(SyncBlock::UnknownChoice(*id)),
        None => Ok(()),
    }
}

/// Folder paths whose rows block or skip every row below them (Y1-D8, Y1-D9).
struct Fences {
    blocked: HashSet<String>,
    skipped: HashSet<String>,
}

impl Fences {
    fn of(request: &SyncRequest<'_>) -> Result<Self, SyncBlock> {
        let mut fences = Self {
            blocked: HashSet::new(),
            skipped: HashSet::new(),
        };
        for (index, row) in request.rows.iter().enumerate() {
            stop_if_cancelled(request, index)?;
            if !is_folder_row(row) {
                continue;
            }
            let choice = request.choices.get(&row.id).copied();
            let set = match decide(row, request.rules, choice) {
                Verdict::Skip if choice == Some(SyncAction::Skip) => &mut fences.skipped,
                Verdict::CantSync(_) => &mut fences.blocked,
                _ => continue,
            };
            set.extend(rel_texts(row));
        }
        Ok(fences)
    }

    fn verdict(&self, row: &Row) -> Option<Verdict> {
        if self.blocked.is_empty() && self.skipped.is_empty() {
            return None;
        }
        let texts = rel_texts(row);
        let below = |set: &HashSet<String>| {
            texts
                .iter()
                .any(|text| ancestors(text).any(|folder| set.contains(folder)))
        };
        if below(&self.blocked) {
            Some(Verdict::CantSync(CantSync::BelowBlocked))
        } else if below(&self.skipped) {
            Some(Verdict::Skip)
        } else {
            None
        }
    }
}

fn is_folder_row(row: &Row) -> bool {
    [row.left.as_deref(), row.right.as_deref()]
        .into_iter()
        .flatten()
        .any(|entry| entry.kind == EntryKind::Folder)
}

fn rel_texts(row: &Row) -> Vec<String> {
    [row.left.as_deref(), row.right.as_deref()]
        .into_iter()
        .flatten()
        .map(|entry| entry.rel.as_path().to_text())
        .collect()
}

/// Every proper ancestor of a relative path: the root `""`, then each prefix before a `/`.
fn ancestors(text: &str) -> impl Iterator<Item = &str> {
    let root = (!text.is_empty()).then_some("");
    root.into_iter().chain(
        text.match_indices('/')
            .filter_map(move |(index, _)| text.get(..index)),
    )
}

struct Sorted<'a> {
    copies: Vec<(&'a Row, Direction)>,
    deletes: Vec<&'a Row>,
    summary: SyncSummary,
}

fn sort_rows<'a>(request: &SyncRequest<'a>, fences: &Fences) -> Result<Sorted<'a>, SyncBlock> {
    let mut sorted = Sorted {
        copies: Vec::new(),
        deletes: Vec::new(),
        summary: SyncSummary::empty(),
    };
    let (mut conflicts, mut checking) = (Vec::new(), Vec::new());
    for (index, row) in request.rows.iter().enumerate() {
        stop_if_cancelled(request, index)?;
        let verdict = fences
            .verdict(row)
            .unwrap_or_else(|| decide(row, request.rules, request.choices.get(&row.id).copied()));
        let summary = &mut sorted.summary;
        match verdict {
            Verdict::Same => summary.same += 1,
            Verdict::Skip => summary.skipped += 1,
            Verdict::Conflict => conflicts.push(row.id),
            Verdict::Waiting => checking.push(row.id),
            Verdict::CantSync(reason) => summary.cant_sync.push(RowRefusal {
                row: row.id,
                reason,
            }),
            Verdict::Copy(direction) => sorted.copies.push((row, direction)),
            Verdict::DeleteTarget => sorted.deletes.push(row),
        }
    }
    if conflicts.is_empty() && checking.is_empty() {
        Ok(sorted)
    } else {
        Err(SyncBlock::Unresolved {
            conflicts,
            checking,
        })
    }
}

impl SyncSummary {
    const fn empty() -> Self {
        Self {
            to_right: parts::no_totals(),
            to_left: parts::no_totals(),
            deleted: parts::no_totals(),
            replaces_newer: 0,
            same: 0,
            skipped: 0,
            cant_sync: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;

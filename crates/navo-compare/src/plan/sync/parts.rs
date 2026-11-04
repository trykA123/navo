use std::collections::{HashMap, HashSet};

use navo_core::{Conflict, Location, OpKind, OperationPlan, Risk, Step, Totals};

use super::super::{
    Parents, Sides, check_name, entries, join_all, plan, segments, side_pair, target_steps,
};
use super::count::{StepCount, kept};
use super::rules::{master, newer};
use super::spelling::{Spellings, respelled};
use super::{
    CantSync, RowRefusal, Sorted, SyncBlock, SyncPlan, SyncRequest, SyncSummary, stop_if_cancelled,
};
use crate::model::{CaseRule, EntryKind, Row};
use crate::session::Refusal;
use crate::text::Direction;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Written {
    Left,
    Right,
}

pub(super) enum Change {
    Copy(Direction),
    Delete,
}

/// One row's planned steps, before collisions and folder sharing are resolved.
pub(super) struct Candidate {
    pub(super) row: u64,
    pub(super) change: Change,
    pub(super) written: Written,
    pub(super) steps: Vec<Step>,
    pub(super) conflicts: Vec<Conflict>,
    pub(super) risks: Vec<Risk>,
    pub(super) destination: Location,
    pub(super) bytes: u64,
    pub(super) replaces_newer: bool,
}

pub(super) const fn no_totals() -> Totals {
    Totals {
        files: 0,
        folders: 0,
        bytes: 0,
    }
}

/// Builds every row's candidate; stops once a part passes `cap` steps or the build is cancelled.
pub(super) fn candidates(
    request: &SyncRequest<'_>,
    sorted: Sorted<'_>,
    present: &HashSet<Location>,
    cap: usize,
) -> Result<(Vec<Candidate>, SyncSummary), SyncBlock> {
    let mut summary = sorted.summary;
    let spellings = Spellings::of(request.rows);
    let mut count = StepCount::new(present, cap);
    let wanted = sorted.copies.len() + sorted.deletes.len();
    let mut out = Vec::with_capacity(wanted.min(cap));
    let copies = sorted
        .copies
        .into_iter()
        .map(|(row, direction)| (row, copy_candidate(row, direction, request, &spellings)));
    let deletes = sorted
        .deletes
        .into_iter()
        .map(|row| (row, delete_candidate(row, request)));
    for (index, (row, candidate)) in copies.chain(deletes).enumerate() {
        stop_if_cancelled(request, index)?;
        match candidate {
            Ok(candidate) => {
                count.add(&candidate)?;
                out.push(candidate);
            }
            Err(reason) => summary.cant_sync.push(RowRefusal {
                row: row.id,
                reason,
            }),
        }
    }
    Ok((out, summary))
}

fn copy_candidate(
    row: &Row,
    direction: Direction,
    request: &SyncRequest<'_>,
    spellings: &Spellings,
) -> Result<Candidate, CantSync> {
    match entries(row, direction) {
        (Some(source), None) if source.kind == EntryKind::Folder => {
            folder_candidate(row, direction, request.sides, spellings).map_err(CantSync::Row)
        }
        _ => file_candidate(row, direction, request, spellings),
    }
}

fn file_candidate(
    row: &Row,
    direction: Direction,
    request: &SyncRequest<'_>,
    spellings: &Spellings,
) -> Result<Candidate, CantSync> {
    let mut planned = plan(
        std::slice::from_ref(row),
        direction,
        request.generation,
        request.sides,
    )
    .map_err(CantSync::Row)?;
    if let (Some(source), None) = entries(row, direction) {
        let names = segments(&source.rel).map_err(CantSync::Row)?;
        if let Some(names) = spellings.respell(written(direction), &names) {
            let (_, target_side) = side_pair(direction, request.sides);
            planned.steps = respelled(planned.steps, &names, target_side).map_err(CantSync::Row)?;
        }
    }
    let destination = planned
        .steps
        .iter()
        .find_map(|step| match step {
            Step::CopyFile { target, .. } => Some(target.clone()),
            _ => None,
        })
        .ok_or(CantSync::Row(Refusal::NotAFile))?;
    let replaces_newer = match (row.left.as_deref(), row.right.as_deref()) {
        (Some(left), Some(right)) => newer(left, right) == Some(reverse(direction)),
        _ => false,
    };
    Ok(Candidate {
        row: row.id,
        change: Change::Copy(direction),
        written: written(direction),
        steps: planned.steps,
        conflicts: planned.conflicts,
        risks: planned.risks,
        destination,
        bytes: planned.totals.bytes,
        replaces_newer,
    })
}

fn folder_candidate(
    row: &Row,
    direction: Direction,
    sides: &Sides,
    spellings: &Spellings,
) -> Result<Candidate, Refusal> {
    let (_, target_side) = side_pair(direction, sides);
    if target_side.read_only {
        return Err(Refusal::ReadOnlyTarget);
    }
    let source = entries(row, direction).0.ok_or(Refusal::NoSource)?;
    let names = segments(&source.rel)?;
    let names = spellings
        .respell(written(direction), &names)
        .unwrap_or(names);
    for name in &names {
        check_name(name, target_side.names)?;
    }
    let (destination, mut steps) = target_steps(&target_side.root, &names, Parents::Make)?;
    steps.push(Step::MakeFolder {
        target: destination.clone(),
    });
    Ok(Candidate {
        row: row.id,
        change: Change::Copy(direction),
        written: written(direction),
        steps,
        conflicts: Vec::new(),
        risks: Vec::new(),
        destination,
        bytes: 0,
        replaces_newer: false,
    })
}

fn delete_candidate(row: &Row, request: &SyncRequest<'_>) -> Result<Candidate, CantSync> {
    let master = master(request.rules.mode).ok_or(CantSync::ChoiceNotAllowed)?;
    let (_, target_side) = side_pair(master, request.sides);
    let (_, target) = entries(row, master);
    let target = target.ok_or(CantSync::Row(Refusal::NoSource))?;
    if target_side.read_only {
        return Err(CantSync::Row(Refusal::ReadOnlyTarget));
    }
    let location = segments(&target.rel)
        .and_then(|names| join_all(&target_side.root, &names))
        .map_err(CantSync::Row)?;
    Ok(Candidate {
        row: row.id,
        change: Change::Delete,
        written: written(master),
        steps: vec![Step::RemoveFile {
            location: location.clone(),
        }],
        conflicts: Vec::new(),
        risks: Vec::new(),
        destination: location,
        bytes: target.size,
        replaces_newer: false,
    })
}

const fn written(direction: Direction) -> Written {
    match direction {
        Direction::LeftToRight => Written::Right,
        Direction::RightToLeft => Written::Left,
    }
}

const fn reverse(direction: Direction) -> Direction {
    match direction {
        Direction::LeftToRight => Direction::RightToLeft,
        Direction::RightToLeft => Direction::LeftToRight,
    }
}

/// Refuses every candidate whose destination another candidate also writes (Y1-D11).
pub(super) fn drop_collisions(
    candidates: Vec<Candidate>,
    sides: &Sides,
    summary: &mut SyncSummary,
) -> Vec<Candidate> {
    let keys: Vec<(Written, String)> = candidates
        .iter()
        .map(|candidate| collision_key(candidate, sides))
        .collect();
    let mut counts: HashMap<&(Written, String), usize> = HashMap::with_capacity(keys.len());
    for key in &keys {
        *counts.entry(key).or_default() += 1;
    }
    let mut kept = Vec::with_capacity(candidates.len());
    for (candidate, key) in candidates.into_iter().zip(&keys) {
        if counts.get(key).copied().unwrap_or(0) > 1 {
            summary.cant_sync.push(RowRefusal {
                row: candidate.row,
                reason: CantSync::PathCollision,
            });
        } else {
            kept.push(candidate);
        }
    }
    kept
}

fn collision_key(candidate: &Candidate, sides: &Sides) -> (Written, String) {
    let side = match candidate.written {
        Written::Left => &sides.left,
        Written::Right => &sides.right,
    };
    let text = candidate.destination.as_str();
    let text = match side.case {
        CaseRule::Insensitive => text.to_lowercase(),
        CaseRule::Sensitive => text.to_owned(),
    };
    (candidate.written, text)
}

/// Orders the candidates into one copy part and one delete part (Y1-D12, Y1-D13).
pub(super) fn assemble(
    candidates: Vec<Candidate>,
    present: &HashSet<Location>,
    mut summary: SyncSummary,
) -> SyncPlan {
    let (mut copies, mut deletes): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|candidate| matches!(candidate.change, Change::Copy(_)));
    copies.sort_by(|a, b| copy_order(a).cmp(&copy_order(b)));
    deletes.sort_by(|a, b| a.destination.as_str().cmp(b.destination.as_str()));
    summary.replaces_newer =
        u64::try_from(copies.iter().filter(|c| c.replaces_newer).count()).unwrap_or(u64::MAX);
    let copy = copy_part(copies, present, &mut summary);
    let delete = delete_part(deletes, &mut summary);
    SyncPlan {
        copy,
        delete,
        summary,
    }
}

fn copy_order(candidate: &Candidate) -> (u8, &str) {
    let rank = match candidate.change {
        Change::Copy(Direction::RightToLeft) => 1,
        _ => 0,
    };
    (rank, candidate.destination.as_str())
}

fn copy_part(
    copies: Vec<Candidate>,
    present: &HashSet<Location>,
    summary: &mut SyncSummary,
) -> Option<OperationPlan> {
    let mut made = HashSet::new();
    let (mut steps, mut conflicts, mut risks) = (Vec::new(), Vec::new(), Vec::new());
    for candidate in copies {
        let totals = match candidate.change {
            Change::Copy(Direction::RightToLeft) => &mut summary.to_left,
            _ => &mut summary.to_right,
        };
        for step in candidate.steps {
            if kept(&step, present, &mut made) {
                count_step(totals, &step);
                steps.push(step);
            }
        }
        conflicts.extend(candidate.conflicts);
        risks.extend(candidate.risks);
    }
    let totals = sum(&summary.to_right, &summary.to_left);
    part(OpKind::Copy, steps, totals).map(|mut plan| {
        plan.conflicts = conflicts;
        plan.risks = risks;
        plan
    })
}

fn delete_part(deletes: Vec<Candidate>, summary: &mut SyncSummary) -> Option<OperationPlan> {
    let mut steps = Vec::with_capacity(deletes.len());
    for candidate in deletes {
        add_file(&mut summary.deleted, candidate.bytes);
        steps.extend(candidate.steps);
    }
    part(OpKind::Delete, steps, summary.deleted.clone())
}

fn part(kind: OpKind, steps: Vec<Step>, totals: Totals) -> Option<OperationPlan> {
    (!steps.is_empty()).then(|| OperationPlan {
        kind,
        steps,
        totals,
        conflicts: Vec::new(),
        risks: Vec::new(),
    })
}

fn count_step(totals: &mut Totals, step: &Step) {
    match step {
        Step::MakeFolder { .. } => totals.folders += 1,
        Step::CopyFile { size, .. } => add_file(totals, *size),
        _ => {}
    }
}

fn add_file(totals: &mut Totals, bytes: u64) {
    totals.files += 1;
    totals.bytes = totals.bytes.saturating_add(bytes);
}

fn sum(a: &Totals, b: &Totals) -> Totals {
    Totals {
        files: a.files + b.files,
        folders: a.folders + b.folders,
        bytes: a.bytes.saturating_add(b.bytes),
    }
}

use super::super::entries;
use crate::model::{EntryKind, Row, SideEntry, Status};
use crate::session::{Refusal, row_write_allowed};
use crate::text::Direction;

/// Two modified times this close (ms) count as the same time.
pub const SAME_TIME_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    TwoWay,
    MirrorRight,
    MirrorLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyNet {
    Kept,
    Missing,
}

/// Deletes apply only in a Mirror mode, and only to target-only files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deletes {
    Off,
    TargetOnly(SafetyNet),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRules {
    pub mode: SyncMode,
    pub deletes: Deletes,
}

/// The user's choice for one row; `Skip` on a folder row skips every row below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    CopyRight,
    CopyLeft,
    Skip,
    DeleteTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Copy(Direction),
    DeleteTarget,
    Same,
    Skip,
    Conflict,
    Waiting,
    CantSync(CantSync),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CantSync {
    Row(Refusal),
    Ignored,
    BelowBlocked,
    ChoiceNotAllowed,
    NoSafetyNet,
    PathCollision,
}

/// The row's own verdict under `rules`, before folder fences and side checks.
#[must_use]
pub fn recommend(row: &Row, rules: SyncRules) -> Verdict {
    decide(row, rules, None)
}

pub(super) fn decide(row: &Row, rules: SyncRules, choice: Option<SyncAction>) -> Verdict {
    if choice == Some(SyncAction::Skip) {
        return Verdict::Skip;
    }
    if row.raw_status == Status::Checking || row.display_status == Status::Checking {
        return Verdict::Waiting;
    }
    if let Some(reason) = blocked(row) {
        return Verdict::CantSync(reason);
    }
    match choice {
        Some(action) => chosen(row, rules, action),
        None => by_mode(row, rules),
    }
}

/// The side a Mirror mode copies from, as a copy direction; `None` for two-way.
pub(super) const fn master(mode: SyncMode) -> Option<Direction> {
    match mode {
        SyncMode::TwoWay => None,
        SyncMode::MirrorRight => Some(Direction::LeftToRight),
        SyncMode::MirrorLeft => Some(Direction::RightToLeft),
    }
}

/// Which side is newer by more than [`SAME_TIME_MS`]; `a` is the left entry.
pub(super) fn newer(a: &SideEntry, b: &SideEntry) -> Option<Direction> {
    let (Some(left), Some(right)) = (a.modified, b.modified) else {
        return None;
    };
    if left.abs_diff(right) <= SAME_TIME_MS {
        None
    } else if left > right {
        Some(Direction::LeftToRight)
    } else {
        Some(Direction::RightToLeft)
    }
}

fn blocked(row: &Row) -> Option<CantSync> {
    if let Err(refusal) = row_write_allowed(row, row.generation) {
        return Some(CantSync::Row(refusal));
    }
    let statuses = [row.raw_status, row.display_status];
    if statuses.contains(&Status::TypeConflict) {
        return Some(CantSync::Row(Refusal::TypeConflict));
    }
    if statuses.contains(&Status::CaseConflict) {
        return Some(CantSync::Row(Refusal::CaseConflict));
    }
    [row.left.as_deref(), row.right.as_deref()]
        .into_iter()
        .flatten()
        .any(|entry| entry.ignored)
        .then_some(CantSync::Ignored)
}

fn chosen(row: &Row, rules: SyncRules, action: SyncAction) -> Verdict {
    match action {
        SyncAction::CopyRight => copy_choice(row, Direction::LeftToRight),
        SyncAction::CopyLeft => copy_choice(row, Direction::RightToLeft),
        SyncAction::DeleteTarget => delete_choice(row, rules),
        SyncAction::Skip => Verdict::Skip,
    }
}

fn copy_choice(row: &Row, direction: Direction) -> Verdict {
    match entries(row, direction) {
        (Some(source), target) if target.is_none() || source.kind == EntryKind::File => {
            Verdict::Copy(direction)
        }
        _ => Verdict::CantSync(CantSync::ChoiceNotAllowed),
    }
}

fn delete_choice(row: &Row, rules: SyncRules) -> Verdict {
    let (Some(master), Deletes::TargetOnly(net)) = (master(rules.mode), rules.deletes) else {
        return Verdict::CantSync(CantSync::ChoiceNotAllowed);
    };
    match entries(row, master) {
        (None, Some(target)) if target.kind == EntryKind::File => delete_verdict(net),
        _ => Verdict::CantSync(CantSync::ChoiceNotAllowed),
    }
}

const fn delete_verdict(net: SafetyNet) -> Verdict {
    match net {
        SafetyNet::Kept => Verdict::DeleteTarget,
        SafetyNet::Missing => Verdict::CantSync(CantSync::NoSafetyNet),
    }
}

fn by_mode(row: &Row, rules: SyncRules) -> Verdict {
    if matches!(
        row.display_status,
        Status::Same | Status::SameAfterRules | Status::SameSizeTime
    ) {
        return Verdict::Same;
    }
    match master(rules.mode) {
        None => two_way(row),
        Some(master) => mirror(row, master, rules.deletes),
    }
}

fn two_way(row: &Row) -> Verdict {
    match (row.left.as_deref(), row.right.as_deref()) {
        (Some(_), None) => Verdict::Copy(Direction::LeftToRight),
        (None, Some(_)) => Verdict::Copy(Direction::RightToLeft),
        (Some(left), Some(right))
            if both_folders(left, right) || same_by_time(row, left, right) =>
        {
            Verdict::Same
        }
        (Some(left), Some(right)) => newer(left, right).map_or(Verdict::Conflict, Verdict::Copy),
        (None, None) => Verdict::CantSync(CantSync::Row(Refusal::NoSource)),
    }
}

fn mirror(row: &Row, master: Direction, deletes: Deletes) -> Verdict {
    match entries(row, master) {
        (Some(_), None) => Verdict::Copy(master),
        (None, Some(target)) => target_only(target, deletes),
        (Some(source), Some(target))
            if both_folders(source, target) || same_by_time(row, source, target) =>
        {
            Verdict::Same
        }
        (Some(_), Some(_)) => Verdict::Copy(master),
        (None, None) => Verdict::CantSync(CantSync::Row(Refusal::NoSource)),
    }
}

fn target_only(target: &SideEntry, deletes: Deletes) -> Verdict {
    match (target.kind, deletes) {
        (EntryKind::File, Deletes::TargetOnly(net)) => delete_verdict(net),
        _ => Verdict::Skip,
    }
}

fn both_folders(a: &SideEntry, b: &SideEntry) -> bool {
    a.kind == EntryKind::Folder && b.kind == EntryKind::Folder
}

/// A `NotChecked` row whose sizes match and whose known times are within tolerance (Y1-D7).
fn same_by_time(row: &Row, a: &SideEntry, b: &SideEntry) -> bool {
    matches!(row.display_status, Status::NotChecked(_))
        && a.size == b.size
        && a.modified.is_some()
        && b.modified.is_some()
        && newer(a, b).is_none()
}

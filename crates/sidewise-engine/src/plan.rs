use navo_core::{
    Conflict, Location, LocationError, OpKind, OperationPlan, ProviderPath, Resolution, Risk, Step,
    Totals,
};

use crate::model::{CaseRule, EntryKind, Generation, RelPath, Row, SideEntry, Status};
use crate::session::{NameProblem, Refusal, row_write_allowed};
use crate::text::Direction;

pub mod sync;

const MAX_WINDOWS_UNITS: usize = 255;
const DOS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// How a side checks the names it writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRules {
    Unix,
    Windows,
}

/// One compare side: where its relative paths hang, its case rule, write policy and name rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    pub root: Location,
    pub case: CaseRule,
    pub read_only: bool,
    pub names: NameRules,
}

/// The two compare sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sides {
    pub left: Side,
    pub right: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parents {
    Make,
    Keep,
}

/// Plans one file row copied toward the other side; `generation` is the session's current one.
/// Without a target entry the full parent chain is planned; the executor skips existing folders.
pub fn plan(
    rows: &[Row],
    direction: Direction,
    generation: Generation,
    sides: &Sides,
) -> Result<OperationPlan, Refusal> {
    let row = only_row(rows)?;
    row_write_allowed(row, generation)?;
    let (source_side, target_side) = side_pair(direction, sides);
    let (source, target) = entries(row, direction);
    let source = source.ok_or(Refusal::NoSource)?;
    check_kinds(row, source, target)?;
    if target_side.read_only {
        return Err(Refusal::ReadOnlyTarget);
    }
    if case_conflict(row, target_side.case) {
        return Err(Refusal::CaseConflict);
    }
    let source_names = segments(&source.rel)?;
    let destination_names = match target {
        Some(entry) => segments(&entry.rel)?,
        None => source_names.clone(),
    };
    for name in &destination_names {
        check_name(name, target_side.names)?;
    }
    let origin = join_all(&source_side.root, &source_names)?;
    let (destination, mut steps) =
        target_steps(&target_side.root, &destination_names, parents_for(target))?;
    let (conflicts, risks) = overwrite(target, &origin, &destination);
    steps.push(Step::CopyFile {
        source: origin,
        target: destination,
        size: source.size,
    });
    let folders = steps
        .iter()
        .filter(|step| matches!(step, Step::MakeFolder { .. }))
        .count();
    Ok(OperationPlan {
        kind: OpKind::Copy,
        steps,
        totals: Totals {
            files: 1,
            folders: u64::try_from(folders).unwrap_or(u64::MAX),
            bytes: source.size,
        },
        conflicts,
        risks,
    })
}

fn only_row(rows: &[Row]) -> Result<&Row, Refusal> {
    match rows {
        [] => Err(Refusal::NoRow),
        [row] => Ok(row),
        _ => Err(Refusal::MultipleRows),
    }
}

const fn side_pair(direction: Direction, sides: &Sides) -> (&Side, &Side) {
    match direction {
        Direction::LeftToRight => (&sides.left, &sides.right),
        Direction::RightToLeft => (&sides.right, &sides.left),
    }
}

fn entries(row: &Row, direction: Direction) -> (Option<&SideEntry>, Option<&SideEntry>) {
    match direction {
        Direction::LeftToRight => (row.left.as_deref(), row.right.as_deref()),
        Direction::RightToLeft => (row.right.as_deref(), row.left.as_deref()),
    }
}

fn check_kinds(row: &Row, source: &SideEntry, target: Option<&SideEntry>) -> Result<(), Refusal> {
    if matches!(row.raw_status, Status::TypeConflict)
        || matches!(row.display_status, Status::TypeConflict)
        || target.is_some_and(|entry| entry.kind != source.kind)
    {
        return Err(Refusal::TypeConflict);
    }
    if source.kind != EntryKind::File {
        return Err(Refusal::NotAFile);
    }
    Ok(())
}

fn parents_for(target: Option<&SideEntry>) -> Parents {
    if target.is_some() {
        Parents::Keep
    } else {
        Parents::Make
    }
}

fn overwrite(
    target: Option<&SideEntry>,
    source: &Location,
    destination: &Location,
) -> (Vec<Conflict>, Vec<Risk>) {
    if target.is_none() {
        return (Vec::new(), Vec::new());
    }
    let conflict = Conflict {
        source: source.clone(),
        target: destination.clone(),
        resolution: Resolution::Overwrite,
    };
    (vec![conflict], vec![Risk::Overwrite(destination.clone())])
}

fn case_conflict(row: &Row, case: CaseRule) -> bool {
    case == CaseRule::Insensitive
        && (matches!(row.raw_status, Status::CaseConflict)
            || matches!(row.display_status, Status::CaseConflict))
}

fn segments(rel: &RelPath) -> Result<Vec<String>, Refusal> {
    let mut names = Vec::new();
    for name in rel.as_path().to_text().split('/') {
        if name.is_empty() {
            return Err(Refusal::NameUnwritable(NameProblem::Empty));
        }
        if name == "." || name == ".." {
            return Err(Refusal::NameUnwritable(NameProblem::DotName));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

fn check_name(name: &str, rules: NameRules) -> Result<(), Refusal> {
    let problem = match rules {
        NameRules::Unix => unix_problem(name),
        NameRules::Windows => windows_problem(name),
    };
    match problem {
        Some(problem) => Err(Refusal::NameUnwritable(problem)),
        None => Ok(()),
    }
}

fn unix_problem(name: &str) -> Option<NameProblem> {
    if name.is_empty() {
        Some(NameProblem::Empty)
    } else if name == "." || name == ".." {
        Some(NameProblem::DotName)
    } else if name.contains('\0') {
        Some(NameProblem::Nul)
    } else {
        None
    }
}

fn windows_problem(name: &str) -> Option<NameProblem> {
    if let Some(problem) = unix_problem(name) {
        return Some(problem);
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Some(NameProblem::TrailingDotOrSpace);
    }
    if name.chars().any(illegal) {
        return Some(NameProblem::IllegalCharacter);
    }
    if name.encode_utf16().count() > MAX_WINDOWS_UNITS {
        return Some(NameProblem::TooLong);
    }
    if is_dos_name(name) {
        return Some(NameProblem::Reserved);
    }
    None
}

fn illegal(character: char) -> bool {
    matches!(character, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*')
        || u32::from(character) < 0x20
}

fn is_dos_name(name: &str) -> bool {
    let stem = name
        .split_once('.')
        .map_or(name, |(stem, _)| stem)
        .trim_end_matches(' ');
    DOS_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

fn join_all(root: &Location, names: &[String]) -> Result<Location, Refusal> {
    let mut folder = root.clone();
    for name in names {
        folder = join_name(&folder, name)?;
    }
    Ok(folder)
}

fn target_steps(
    root: &Location,
    names: &[String],
    parents: Parents,
) -> Result<(Location, Vec<Step>), Refusal> {
    let Some((name, folders)) = names.split_last() else {
        return Err(Refusal::NameUnwritable(NameProblem::Empty));
    };
    let mut folder = root.clone();
    let mut steps = Vec::with_capacity(names.len());
    for parent in folders {
        folder = join_name(&folder, parent)?;
        if matches!(parents, Parents::Make) {
            steps.push(Step::MakeFolder {
                target: folder.clone(),
            });
        }
    }
    let destination = join_name(&folder, name)?;
    Ok((destination, steps))
}

fn join_name(folder: &Location, name: &str) -> Result<Location, Refusal> {
    folder
        .join(&ProviderPath::from_text(name))
        .map_err(|error| Refusal::NameUnwritable(problem_of(error)))
}

fn problem_of(error: LocationError) -> NameProblem {
    match error {
        LocationError::Empty => NameProblem::Empty,
        LocationError::Nul => NameProblem::Nul,
        LocationError::BadProvider | LocationError::BadSegment => NameProblem::IllegalCharacter,
    }
}

#[cfg(test)]
mod tests;

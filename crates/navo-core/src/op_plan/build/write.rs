// Plan builder for saving one staged file, or many in one job.
// Every step writes a staged copy over a target after a stamp check.

use crate::Location;
use crate::op_plan::{FileStamp, OpKind, OperationPlan, Risk, Step, Totals};

/// One file of a multi-file save: the staged copy, its target and the target's stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTarget {
    pub target: Location,
    pub staged: Location,
    pub expect: FileStamp,
    pub staged_len: u64,
}

/// Why a save plan over several files was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteManyError {
    #[error("a save plan needs at least one file")]
    Empty,
    #[error("the save plan uses this path twice: {}", .0.as_str())]
    Duplicate(Location),
}

/// Builds a one-file Save plan that writes the staged copy over the target.
#[must_use]
pub fn write_plan(
    target: &Location,
    staged: &Location,
    expect: FileStamp,
    staged_len: u64,
) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Save,
        steps: vec![Step::WriteFile {
            target: target.clone(),
            staged: staged.clone(),
            staged_len,
            expect,
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: staged_len,
        },
        conflicts: Vec::new(),
        risks: vec![Risk::Overwrite(target.clone())],
    }
}

/// Builds one Save plan with a WriteFile step per target.
/// Refuses an empty list, or a target or staged path that appears twice.
pub fn write_many_plan(writes: &[WriteTarget]) -> Result<OperationPlan, WriteManyError> {
    if writes.is_empty() {
        return Err(WriteManyError::Empty);
    }
    refuse_repeated_paths(writes)?;
    let mut steps = Vec::with_capacity(writes.len());
    let mut risks = Vec::with_capacity(writes.len());
    let mut bytes = 0_u64;
    for write in writes {
        bytes = bytes.saturating_add(write.staged_len);
        steps.push(Step::WriteFile {
            target: write.target.clone(),
            staged: write.staged.clone(),
            staged_len: write.staged_len,
            expect: write.expect,
        });
        risks.push(Risk::Overwrite(write.target.clone()));
    }
    let files = u64::try_from(writes.len()).unwrap_or(u64::MAX);
    Ok(OperationPlan {
        kind: OpKind::Save,
        steps,
        totals: Totals {
            files,
            folders: 0,
            bytes,
        },
        conflicts: Vec::new(),
        risks,
    })
}

/// Refuses a target or staged path that another target or staged path already uses.
fn refuse_repeated_paths(writes: &[WriteTarget]) -> Result<(), WriteManyError> {
    let mut seen: Vec<&Location> = Vec::with_capacity(writes.len() * 2);
    for write in writes {
        for path in [&write.target, &write.staged] {
            if seen.iter().any(|other| other.same_place(path)) {
                return Err(WriteManyError::Duplicate(path.clone()));
            }
            seen.push(path);
        }
    }
    Ok(())
}

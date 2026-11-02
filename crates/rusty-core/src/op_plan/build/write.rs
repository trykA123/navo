use crate::Location;
use crate::op_plan::{FileStamp, OpKind, OperationPlan, Risk, Step, Totals};

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

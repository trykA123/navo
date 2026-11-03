use navo_core::Totals;

use super::*;

#[test]
fn announced_sizes_that_overflow_saturate_the_byte_total() {
    let fs = fixture(&[]);
    let step = |name: &str| Step::CopyFile {
        source: fs.location(&format!("src/{name}")),
        target: fs.location(&format!("dst/{name}")),
        size: u64::MAX,
    };
    let plan = OperationPlan {
        kind: OpKind::Copy,
        steps: vec![step("a.bin"), step("b.bin")],
        totals: Totals {
            files: 2,
            folders: 0,
            bytes: u64::MAX,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    assert_eq!(Progress::start(&plan).bytes_total, u64::MAX);
}

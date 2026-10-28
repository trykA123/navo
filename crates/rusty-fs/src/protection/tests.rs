use rusty_core::{FsErrorKind, OpKind, OperationPlan, Step, Totals};

use super::*;
use crate::{FakeFs, FakeOp, Fault};

fn plan(kind: OpKind, steps: Vec<Step>) -> OperationPlan {
    OperationPlan {
        kind,
        steps,
        totals: Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

fn moves(fs: &FakeFs, source: &str) -> Vec<OperationPlan> {
    vec![
        plan(
            OpKind::Recycle,
            vec![Step::Recycle {
                location: fs.location(source),
            }],
        ),
        plan(
            OpKind::Move,
            vec![Step::Rename {
                source: fs.location(source),
                target: fs.location("dest/moved"),
            }],
        ),
        plan(
            OpKind::Rename,
            vec![Step::Rename {
                source: fs.location(source),
                target: fs.location("home/renamed"),
            }],
        ),
    ]
}

#[test]
fn relocating_a_folder_with_a_deep_reserved_folder_is_refused() {
    for reserved in [
        ".navo-retained-ab",
        ".ferrite-held-1-2",
        ".NAVO-MUTATIONS-x",
    ] {
        let fs = FakeFs::new();
        fs.add_file(&format!("home/docs/deep/er/{reserved}/payload/a"), 2);
        fs.add_folder("dest");
        for plan in moves(&fs, "home/docs") {
            let error = check_relocation(&plan, &fs, &CancelToken::new()).unwrap_err();
            assert_eq!(error.kind, FsErrorKind::AccessDenied, "{reserved}");
        }
    }
}

#[test]
fn relocating_ordinary_folders_and_files_passes() {
    let fs = FakeFs::new();
    fs.add_file("home/docs/sub/navo-retained-lookalike/a", 2);
    fs.add_file("home/docs/b", 2);
    fs.add_file("home/file", 2);
    fs.add_folder("dest");
    for source in ["home/docs", "home/file", "home/missing"] {
        for plan in moves(&fs, source) {
            check_relocation(&plan, &fs, &CancelToken::new()).unwrap();
        }
    }
}

#[test]
fn relocation_lists_nothing_for_files_or_permanent_deletes() {
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 2);
    let steps = vec![
        Step::RemoveFile {
            location: fs.location("home/docs/a"),
        },
        Step::RemoveFolder {
            location: fs.location("home/docs"),
        },
    ];
    check_relocation(&plan(OpKind::Delete, steps), &fs, &CancelToken::new()).unwrap();
    for plan in moves(&fs, "home/docs/a") {
        check_relocation(&plan, &fs, &CancelToken::new()).unwrap();
    }
    assert_eq!(fs.calls("home/docs", FakeOp::List), 0);
}

#[test]
fn relocation_stops_at_the_first_reserved_name() {
    let fs = FakeFs::new();
    fs.add_file("home/docs/.navo-retained-ab/payload/a", 2);
    fs.add_folder("home/docs/.navo-retained-ab/payload/deeper");
    for plan in moves(&fs, "home/docs") {
        assert!(check_relocation(&plan, &fs, &CancelToken::new()).is_err());
    }
    assert_eq!(
        fs.calls("home/docs/.navo-retained-ab", FakeOp::List)
            + fs.calls("home/docs/.navo-retained-ab/payload", FakeOp::List),
        0
    );
}

#[test]
fn an_unreadable_folder_or_cancel_refuses_the_relocation() {
    let fs = FakeFs::new();
    fs.add_folder("home/docs/locked");
    fs.add_folder("dest");
    fs.fault(
        "home/docs/locked",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let recycle = &moves(&fs, "home/docs")[0];
    assert!(check_relocation(recycle, &fs, &CancelToken::new()).is_err());
    let cancel = CancelToken::new();
    cancel.cancel();
    assert_eq!(
        check_relocation(recycle, &fs, &cancel).unwrap_err().kind,
        FsErrorKind::Cancelled
    );
}

#[test]
fn overlapping_sources_list_each_descendant_folder_once() {
    for mutation in [false, true] {
        let fs = FakeFs::new();
        fs.add_file("home/docs/sub/deep/a", 1);
        fs.add_file("other/b", 1);
        let mut steps = vec![];
        for source in ["home/docs", "home/docs/sub", "home/docs", "other"] {
            steps.push(Step::Rename {
                source: fs.location(source),
                target: fs.location("dest/moved"),
            });
        }
        steps.push(Step::Recycle {
            location: fs.location("home/docs/sub/deep"),
        });
        steps.push(Step::RemoveFolder {
            location: fs.location("home/docs/sub"),
        });
        let plan = plan(OpKind::Move, steps);
        let check = if mutation {
            check_mutation
        } else {
            check_relocation
        };
        check(&plan, &fs, &CancelToken::new()).unwrap();
        for folder in ["home/docs", "home/docs/sub", "home/docs/sub/deep", "other"] {
            assert_eq!(fs.calls(folder, FakeOp::List), 1, "{folder}");
        }
    }
}

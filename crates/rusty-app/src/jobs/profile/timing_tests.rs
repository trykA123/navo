use std::path::Path;
use std::time::Instant;

use rusty_core::{ConflictPolicy, PlanRequest, plan};
use rusty_fs::{CancelToken, LocalFs, scan};

use super::*;

fn at(path: &Path) -> Location {
    crate::startup::path_location(path).unwrap()
}

fn fixture(root: &Path, folders: usize, files: usize) -> (Location, StoreDirs) {
    let dirs = StoreDirs::in_profile(&root.join("profile"));
    dirs.ensure().unwrap();
    for folder in 0..folders {
        let parent = root.join("source").join(format!("folder-{folder}"));
        std::fs::create_dir_all(&parent).unwrap();
        for file in 0..files {
            std::fs::write(parent.join(format!("file-{file}")), b"").unwrap();
        }
    }
    std::fs::create_dir(root.join("target")).unwrap();
    (at(&root.join("source")), dirs)
}

fn measure(kind: OpKind, folders: usize, files: usize) {
    let root = tempfile::tempdir().unwrap();
    let (source, dirs) = fixture(root.path(), folders, files);
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let plan = plan(&PlanRequest {
        kind,
        items: scan(&ops, &[source], &cancel).unwrap(),
        target: Some(at(&root.path().join("target"))),
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    });
    assert_eq!(plan.totals.files, u64::try_from(folders * files).unwrap());
    for experimental in [false, true] {
        let started = Instant::now();
        guard(&plan, &dirs, experimental).unwrap();
        if experimental {
            rusty_fs::check_mutation(&plan, &ops, &cancel).unwrap();
        } else {
            rusty_fs::check_relocation(&plan, &ops, &cancel).unwrap();
        }
        eprintln!(
            "pre-check {kind:?}, {} files, retention={experimental}: {:?}",
            folders * files,
            started.elapsed()
        );
    }
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn copy_100k_files_prechecks() {
    measure(OpKind::Copy, 100, 1_000);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn same_drive_move_20k_files_prechecks() {
    measure(OpKind::Move, 100, 200);
}

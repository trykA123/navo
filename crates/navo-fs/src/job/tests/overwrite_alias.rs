//! Red tests: a target that is a second name of its source (mapped drive and its share, SUBST,
//! a mount point, a hard link). Paths differ, so the planner and the job treat it as another file.

#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
use navo_core::{ExistingItem, ScannedItem};

#[cfg(unix)]
use super::cross_provider::local;
use super::*;
#[cfg(unix)]
use crate::LocalFs;

const BYTES: &[u8] = b"the only copy";

/// A move or copy of `home/a.txt` into `dest` with Overwrite all, `dest/a.txt` already listed.
#[cfg(unix)]
fn alias_plan(kind: OpKind, home: &Path, dest: &Path) -> OperationPlan {
    let size = BYTES.len() as u64;
    plan(&PlanRequest {
        kind,
        items: vec![ScannedItem {
            location: local(&home.join("a.txt")),
            depth: 0,
            kind: ItemKind::File,
            size,
            modified_ms: None,
        }],
        target: Some(local(dest)),
        existing: vec![ExistingItem {
            location: local(&dest.join("a.txt")),
            kind: ItemKind::File,
            size,
            modified_ms: None,
        }],
        policy: ConflictPolicy::OverwriteAll,
        same_volume: false,
        protected: Vec::new(),
    })
}

/// `home` with `a.txt`, and `dest`: a second name of `home`, as a SUBST drive or a mapped drive
/// next to its UNC share is on Windows.
#[cfg(unix)]
fn aliased() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let (home, dest) = (dir.path().join("home"), dir.path().join("dest"));
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("a.txt"), BYTES).unwrap();
    std::os::unix::fs::symlink(&home, &dest).unwrap();
    (dir, home, dest)
}

/// Job options with `workers` and nothing else switched on.
fn options(workers: usize) -> JobOptions<'static> {
    JobOptions {
        workers,
        sources: None,
        verify: None,
        speed_cap: None,
    }
}

/// What survives under `home`: the file's bytes, or `None` when it is gone.
#[cfg(unix)]
fn survivor(home: &Path) -> Option<Vec<u8>> {
    std::fs::read(home.join("a.txt")).ok()
}

/// The names left in `home`, so a failure shows whether a held old copy survived.
#[cfg(unix)]
fn listing(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(home)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
#[ignore = "red: a move with Overwrite onto an alias of its source copies the file onto itself \
            and then deletes the only copy"]
fn a_move_with_overwrite_onto_an_alias_of_its_source_keeps_the_file() {
    for workers in MODES {
        let (_dir, home, dest) = aliased();
        let plan = alias_plan(OpKind::Move, &home, &dest);
        let outcome = Harness::default().with(&LocalFs::new(), |ctx| {
            run_job_with(&plan, &[], ctx, options(workers))
        });
        assert_eq!(
            survivor(&home).as_deref(),
            Some(BYTES),
            "workers {workers}: the move ended {outcome:?} and a.txt is gone; left in home: {:?}",
            listing(&home)
        );
    }
}

#[cfg(unix)]
#[test]
fn a_copy_with_overwrite_onto_an_alias_of_its_source_keeps_the_bytes() {
    for workers in MODES {
        let (_dir, home, dest) = aliased();
        let plan = alias_plan(OpKind::Copy, &home, &dest);
        let outcome = Harness::default().with(&LocalFs::new(), |ctx| {
            run_job_with(&plan, &[], ctx, options(workers))
        });
        assert_eq!(
            survivor(&home).as_deref(),
            Some(BYTES),
            "workers {workers}: the copy ended {outcome:?}"
        );
    }
}

#[test]
#[ignore = "red: the job overwrites a target whose file identity is the source's own"]
fn a_move_refuses_a_target_with_the_source_identity() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", BIG), ("dst/a.txt", BIG)]);
        fs.share_identity("dst/a.txt", "src/a.txt");
        let plan = plan(&PlanRequest {
            policy: ConflictPolicy::OverwriteAll,
            ..request(&fs, OpKind::Move, &["src/a.txt"], "dst")
        });
        let outcome =
            Harness::default().with(&fs, |ctx| run_job_with(&plan, &[], ctx, options(workers)));
        assert!(
            fs.metadata(&fs.location("src/a.txt")).is_ok(),
            "workers {workers}: the move ended {outcome:?} and removed a source whose identity \
             is the target's"
        );
    }
}

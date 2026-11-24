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

#[cfg(unix)]
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
fn a_move_with_overwrite_onto_an_alias_of_its_source_keeps_the_file() {
    for (workers, same_volume) in [(0, false), (4, false), (0, true)] {
        let (_dir, home, dest) = aliased();
        let mut plan = alias_plan(OpKind::Move, &home, &dest);
        if same_volume {
            plan = rename_plan(&home, &dest);
        }
        let outcome = Harness::default().with(&LocalFs::new(), |ctx| {
            run_job_with(&plan, &[], ctx, options(workers))
        });
        assert_eq!(
            survivor(&home).as_deref(),
            Some(BYTES),
            "workers {workers}: the move ended {outcome:?} and a.txt is gone; left in home: {:?}",
            listing(&home)
        );
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::SameFile)
        );
        assert_eq!(
            listing(&home),
            ["a.txt"],
            "workers {workers}: nothing is held aside"
        );
    }
}

/// The same move planned on one volume: a rename over the alias instead of a copy.
#[cfg(unix)]
fn rename_plan(home: &Path, dest: &Path) -> OperationPlan {
    let mut plan = alias_plan(OpKind::Move, home, dest);
    plan.steps = vec![Step::Rename {
        source: local(&home.join("a.txt")),
        target: local(&dest.join("a.txt")),
    }];
    plan
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
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::SameFile)
        );
    }
}

/// `src/a.txt` and a smaller `dst/a.txt`, so an overwrite shows in the target's size.
fn two_files() -> FakeFs {
    fixture(&[("src/a.txt", BIG), ("dst/a.txt", 7)])
}

/// Runs a copy of `src/a.txt` into `dst` with Overwrite all.
fn copy_over(fs: &FakeFs, workers: usize) -> JobOutcome {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        ..request(fs, OpKind::Copy, &["src/a.txt"], "dst")
    });
    Harness::default().with(fs, |ctx| run_job_with(&plan, &[], ctx, options(workers)))
}

#[test]
fn a_copy_onto_its_own_alias_is_refused_and_changes_nothing() {
    for workers in MODES {
        let fs = two_files();
        fs.share_identity("dst/a.txt", "src/a.txt");
        let before = (tree_of(&fs, "src"), tree_of(&fs, "dst"));
        let outcome = copy_over(&fs, workers);
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::SameFile)
        );
        assert_eq!(
            (tree_of(&fs, "src"), tree_of(&fs, "dst")),
            before,
            "workers {workers}"
        );
    }
}

#[test]
fn a_different_file_at_the_target_is_still_overwritten() {
    for workers in MODES {
        let fs = two_files();
        let outcome = copy_over(&fs, workers);
        assert_eq!(outcome, JobOutcome::Done, "workers {workers}");
        assert_eq!(size(&fs, "dst/a.txt"), Some(BIG), "workers {workers}");
    }
}

#[test]
fn a_target_without_a_readable_identity_is_refused() {
    for workers in MODES {
        let fs = two_files();
        fs.set_volume_serial("dst", None);
        let before = tree_of(&fs, "dst");
        let outcome = copy_over(&fs, workers);
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::IdentityUnavailable),
            "workers {workers}"
        );
        assert_eq!(tree_of(&fs, "dst"), before, "workers {workers}");
    }
}

/// `two_files` on a volume that keeps no file IDs, as FAT or exFAT does.
fn without_ids() -> FakeFs {
    let fs = two_files();
    fs.set_without_ids("src");
    fs.set_without_ids("dst");
    fs
}

#[test]
fn without_ids_another_volume_serial_still_overwrites() {
    for workers in MODES {
        let fs = without_ids();
        fs.set_volume("dst", 7);
        assert_eq!(
            copy_over(&fs, workers),
            JobOutcome::Done,
            "workers {workers}"
        );
        assert_eq!(size(&fs, "dst/a.txt"), Some(BIG), "workers {workers}");
    }
}

#[test]
fn without_ids_on_one_volume_another_path_still_overwrites() {
    for workers in MODES {
        let fs = without_ids();
        assert_eq!(
            copy_over(&fs, workers),
            JobOutcome::Done,
            "workers {workers}"
        );
        assert_eq!(size(&fs, "dst/a.txt"), Some(BIG), "workers {workers}");
    }
}

#[test]
fn without_ids_on_one_volume_the_same_resolved_path_is_refused() {
    for workers in MODES {
        let fs = without_ids();
        fs.set_alias("dst", "src");
        let before = (tree_of(&fs, "src"), tree_of(&fs, "dst"));
        let outcome = copy_over(&fs, workers);
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::SameFile)
        );
        assert_eq!(
            (tree_of(&fs, "src"), tree_of(&fs, "dst")),
            before,
            "workers {workers}"
        );
    }
}

#[test]
fn without_ids_an_unreadable_volume_serial_is_refused() {
    for workers in MODES {
        let fs = without_ids();
        fs.set_volume_serial("src", None);
        let before = tree_of(&fs, "dst");
        let outcome = copy_over(&fs, workers);
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::IdentityUnavailable),
            "workers {workers}"
        );
        assert_eq!(tree_of(&fs, "dst"), before, "workers {workers}");
    }
}

#[cfg(unix)]
#[test]
fn a_local_alias_resolves_to_its_source_path() {
    let (_dir, home, dest) = aliased();
    let fs = LocalFs::new();
    let real = |at: &Path| fs.canonical(&local(&at.join("a.txt"))).unwrap().unwrap();
    assert!(real(&dest).same_place(&real(&home)));
}

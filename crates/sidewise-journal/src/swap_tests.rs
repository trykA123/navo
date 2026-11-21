//! Same-size, same-time swaps of a target that only a hash can see (NC160, NC202): during an
//! undo, before its backup or before its replace, and during the restore copy of a rollback.
// Every platform, except the recovery case, which needs the Linux `FaultDisk` crash.

use std::path::PathBuf;
use std::sync::Arc;

use navo_core::TEMP_SUFFIX;
use sidewise_fs::durable::RealDisk;

use crate::noids_tests::{Files, Ids, NEW, NoIds, Swap, files, is_backup, state, temps, write_new};
use crate::{Expect, JobKind, Journal, UndoError};

/// Other bytes of the same length as `NEW`, so a size check alone cannot see the swap.
const SWAP2: &[u8] = b"his new bytes";

/// True for a staged temp, `.sw-<job>-<item>.navo-part`.
fn is_staged(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(TEMP_SUFFIX) else {
        return false;
    };
    stem.starts_with(".sw-") && !stem.ends_with("-r") && !stem.ends_with("-old")
}

/// Overwrites one target with `NEW` and commits, then arms a `SWAP2` swap that fires after
/// the create `on` matches, and undoes the job. Returns the undo result, the files, the
/// target's modified time before the swap and how often the swapped target was replaced.
fn undo_with_swap(
    ids: Ids,
    on: fn(&str) -> bool,
) -> (Result<Vec<PathBuf>, UndoError>, Files, Option<i64>, usize) {
    let files = files(1, None);
    let target = files.targets[0].clone();
    let swap = Swap {
        path: target.clone(),
        bytes: SWAP2,
        on,
    };
    let disk = Arc::new(NoIds::new(Arc::new(RealDisk), ids, Some(swap)));
    let journal = Journal::open_raw(files.journal.path(), disk.clone()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    job.overwrite(&target, Expect::default(), None, &mut write_new)
        .unwrap();
    let id = job.commit().unwrap();
    let (bytes, new_ms) = state(&target);
    assert_eq!(bytes, NEW);
    disk.arm();
    let result = journal.undo(&id);
    let replaced = disk.state().unwrap().replaced;
    (result, files, new_ms, replaced)
}

#[test]
fn an_undo_never_replaces_a_same_size_same_time_edit_made_before_its_backup() {
    for ids in [Ids::Kept, Ids::Hidden] {
        let (result, files, new_ms, replaced) = undo_with_swap(ids, is_staged);
        let target = files.targets[0].clone();
        assert_eq!(result, Err(UndoError::Changed(target.clone())));
        assert_eq!(replaced, 0, "the target was never written");
        assert_eq!(state(&target), (SWAP2.to_vec(), new_ms), "the edit is kept");
        assert!(temps(&files).is_empty());
    }
}

#[test]
fn an_undo_without_ids_refuses_a_same_length_swap_before_its_replace() {
    let (result, files, new_ms, replaced) = undo_with_swap(Ids::Hidden, is_backup);
    let target = files.targets[0].clone();
    assert_eq!(result, Err(UndoError::Changed(target.clone())));
    assert_eq!(replaced, 0, "the target was never written");
    assert_eq!(state(&target), (SWAP2.to_vec(), new_ms), "the edit is kept");
    assert!(temps(&files).is_empty());
}

/// Recovery at open (J3) without ids: a swap during the restore copy is kept by hash.
#[cfg(target_os = "linux")]
mod recover {
    use std::fs;
    use std::sync::Arc;

    use sidewise_fs::durable::RealDisk;

    use crate::job_tests::{Fixture, NEW, OLD, mtime_ms};
    use crate::noids_tests::{Ids, NoIds, Swap};
    use crate::recover_tests::crashed_open_job;
    use crate::{Journal, Recovered};

    /// Other bytes of the same length as the fixture's `NEW`.
    const SAVED: &[u8] = b"his new bytes, longer\n";

    /// True for a restore temp, `…-r.navo-part`.
    fn is_restore(name: &str) -> bool {
        name.ends_with(&format!("-r{}", navo_core::TEMP_SUFFIX))
    }

    #[test]
    fn recover_without_ids_keeps_a_same_size_same_time_swap_during_the_restore_copy() {
        assert_eq!(SAVED.len(), NEW.len());
        let fixture = Fixture::new(1);
        crashed_open_job(&fixture, 1);
        let target = fixture.targets[0].clone();
        let new_ms = mtime_ms(&target);
        let swap = Swap {
            path: target.clone(),
            bytes: SAVED,
            on: is_restore,
        };
        let disk = Arc::new(NoIds::new(Arc::new(RealDisk), Ids::Hidden, Some(swap)));
        disk.arm();
        let (_journal, recovered) = Journal::open(&fixture.journal_dir(), disk.clone()).unwrap();
        let want = Recovered {
            kept: 1,
            rolled_back: 1,
            swept: recovered.swept,
            ..Recovered::default()
        };
        assert_eq!(recovered, want);
        assert_eq!(
            disk.state().unwrap().replaced,
            0,
            "the target was never written"
        );
        assert_eq!(fs::read(&target).unwrap(), SAVED, "the swap is kept");
        assert_eq!(mtime_ms(&target), new_ms, "its modified time is kept");
        assert_eq!(fs::read(fixture.jobs()[0].join("0.bak")).unwrap(), OLD);
        assert_eq!(
            fixture.temps(),
            Vec::<String>::new(),
            "no restore temp left"
        );
    }
}

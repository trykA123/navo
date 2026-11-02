use rusty_core::{FileStamp, OperationPlan, write_plan};

use super::kept_aside::{entry_at, holding, keeping};
use super::*;

pub(super) const TARGET: &str = "dest/a.txt";
pub(super) const STAGED: &str = "dest/.a.txt.part";
pub(super) const OLD: u64 = 3;
pub(super) const NEW: u64 = 11;
pub(super) const OLD_HASH: [u8; 32] = [1; 32];
pub(super) const NEW_HASH: [u8; 32] = [2; 32];

fn file(name: &str, (size, modified): (u64, i64)) -> FsEntry {
    FsEntry {
        name: rusty_core::ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms: Some(modified),
        attributes: Attributes::default(),
    }
}

pub(super) fn put(fs: &FakeFs, path: &str, (size, modified): (u64, i64), hash: [u8; 32]) {
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.add_entry(folder, file(name, (size, modified)));
    fs.set_hash(path, hash);
}

pub(super) fn seen() -> FileStamp {
    FileStamp {
        size: OLD,
        modified_ms: Some(1_000),
        hash: OLD_HASH,
    }
}

pub(super) fn saving(staged: &str, expect: FileStamp) -> (FakeFs, OperationPlan) {
    let fs = FakeFs::new();
    fs.add_folder("dest");
    put(&fs, TARGET, (OLD, 1_000), OLD_HASH);
    if let Some((folder, _)) = staged.rsplit_once('/') {
        fs.add_folder(folder);
    }
    put(&fs, staged, (NEW, 2_000), NEW_HASH);
    let plan = write_plan(&fs.location(TARGET), &fs.location(staged), expect, NEW);
    (fs, plan)
}

pub(super) fn hash_at(fs: &FakeFs, path: &str) -> [u8; 32] {
    fs.content_hash(&fs.location(path), &CancelToken::new())
        .expect("hashable file")
}

pub(super) fn untouched(fs: &FakeFs, staged: &str) -> bool {
    size(fs, TARGET) == Some(OLD)
        && hash_at(fs, TARGET) == OLD_HASH
        && size(fs, staged) == Some(NEW)
        && holding(fs).is_empty()
}

fn refused_with(expect: FileStamp, kind: FsErrorKind) {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, expect);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(failure(&outcome), Some((0, kind)), "workers {workers}");
        assert!(untouched(&fs, STAGED), "workers {workers}");
        assert!(harness.log.held.is_empty(), "workers {workers}");
    }
}

#[test]
fn a_changed_size_is_refused() {
    refused_with(
        FileStamp {
            size: OLD + 1,
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
}

#[test]
fn a_changed_time_is_refused() {
    refused_with(
        FileStamp {
            modified_ms: Some(999),
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
}

#[test]
fn a_change_seen_only_in_the_hash_is_refused() {
    refused_with(
        FileStamp {
            hash: [7; 32],
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
}

#[test]
fn a_missing_target_is_refused() {
    let (fs, plan) = saving(STAGED, seen());
    fs.remove(TARGET);
    let (outcome, _) = run(&fs, &plan);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert_eq!(size(&fs, STAGED), Some(NEW));
}

#[test]
fn a_staged_file_of_another_length_is_refused() {
    let (fs, plan) = saving(STAGED, seen());
    put(&fs, STAGED, (NEW + 1, 2_000), NEW_HASH);
    let (outcome, _) = run(&fs, &plan);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert_eq!(size(&fs, TARGET), Some(OLD));
}

#[test]
fn a_staged_file_on_another_volume_is_refused() {
    for workers in MODES {
        let staged = "staging/a.txt.part";
        let (fs, plan) = saving(staged, seen());
        fs.set_volume("staging", 9);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            failure(&outcome),
            Some((0, FsErrorKind::NotSupportedYet)),
            "workers {workers}"
        );
        assert!(untouched(&fs, staged), "workers {workers}");
    }
}

#[test]
fn a_staged_file_on_another_mount_is_refused() {
    let staged = "staging/a.txt.part";
    let (fs, plan) = saving(staged, seen());
    fs.set_mount_identity("staging", Some(5));
    let (outcome, _) = run(&fs, &plan);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::NotSupportedYet)));
    assert!(untouched(&fs, staged));
}

#[test]
fn a_write_places_the_staged_file_and_holds_the_old_one() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let mut harness = keeping(workers);
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        assert_eq!(harness.log.done(), [0], "workers {workers}");
        assert_eq!(hash_at(&fs, TARGET), NEW_HASH, "workers {workers}");
        assert!(!exists(&fs, STAGED), "workers {workers}");
        let [held] = harness.log.held.as_slice() else {
            panic!("one held file, workers {workers}");
        };
        let old = held.held.path().to_text();
        assert_eq!(hash_at(&fs, &old), OLD_HASH, "workers {workers}");
        assert_eq!(entry_at(&fs, &old).map(|e| e.size), Some(OLD));
    }
}

#[test]
fn a_finished_write_releases_the_old_file() {
    let (fs, plan) = saving(STAGED, seen());
    let (outcome, _) = run(&fs, &plan);
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(hash_at(&fs, TARGET), NEW_HASH);
    assert!(holding(&fs).is_empty());
    assert_eq!(tree_of(&fs, "dest"), [entry("a.txt", EntryKind::File, NEW)]);
}

#[test]
fn a_cancel_while_reading_the_target_leaves_the_old_file() {
    let (fs, plan) = saving(STAGED, seen());
    fs.fault(TARGET, FakeOp::Read, 1, Fault::Fail(FsErrorKind::Cancelled));
    let (outcome, harness) = run(&fs, &plan);
    assert_eq!(outcome, JobOutcome::Cancelled);
    assert!(untouched(&fs, STAGED));
    assert!(harness.log.done().is_empty());
}

#[test]
fn a_cancel_after_the_hold_puts_the_old_file_back() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let mut harness = keeping(workers);
        let ops =
            super::crashing::Crashing::cancelling(&fs, TARGET, FakeOp::Rename, &harness.cancel);
        assert_eq!(harness.run(&ops, &plan, &[]), JobOutcome::Cancelled);
        assert_eq!(hash_at(&fs, TARGET), OLD_HASH, "workers {workers}");
        assert_eq!(size(&fs, STAGED), Some(NEW), "workers {workers}");
        assert!(harness.log.done().is_empty(), "workers {workers}");
        assert!(harness.log.stranded.is_empty(), "workers {workers}");
    }
}

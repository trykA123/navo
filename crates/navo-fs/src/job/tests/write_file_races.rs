use std::sync::{Arc, Mutex};

use super::crashing::Crashing;
use super::kept_aside::keeping;
use super::write_file::{
    NEW, NEW_HASH, OLD, OLD_HASH, STAGED, TARGET, hash_at, put, saving, seen, untouched,
};
use super::*;

fn refused_by(meddle: impl Fn(&FakeFs) + Send + Sync + 'static, at: (&str, FakeOp)) -> FakeFs {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = keeping(0);
    let ops = Crashing::meddling(&fs, at, meddle);
    let outcome = harness.run(&ops, &plan, &[]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert!(harness.log.stranded.is_empty());
    assert!(!exists(&fs, &harness.log.held[0].held.path().to_text()));
    assert!(harness.log.done().is_empty());
    fs
}

#[test]
fn a_target_rewritten_after_the_hash_with_the_same_size_and_time_is_put_back() {
    let fs = refused_by(
        |fs| put(fs, TARGET, (OLD, 1_000), [8; 32]),
        (STAGED, FakeOp::Read),
    );
    assert_eq!(hash_at(&fs, TARGET), [8; 32]);
    assert_eq!(hash_at(&fs, STAGED), NEW_HASH);
}

#[test]
fn a_target_replaced_by_another_file_after_the_hash_is_put_back() {
    let fs = refused_by(
        |fs| {
            fs.remove(TARGET);
            put(fs, TARGET, (OLD, 1_000), OLD_HASH);
        },
        (STAGED, FakeOp::Read),
    );
    assert_eq!(size(&fs, TARGET), Some(OLD));
    assert_eq!(size(&fs, STAGED), Some(NEW));
}

#[test]
fn a_staged_file_turned_into_a_link_during_the_hold_is_refused() {
    let fs = refused_by(
        |fs| {
            fs.remove(STAGED);
            add_link(fs, "dest", ".a.txt.part");
        },
        (TARGET, FakeOp::Rename),
    );
    assert!(untouched_old(&fs));
}

#[test]
fn a_staged_file_of_another_length_after_the_hold_is_refused() {
    let fs = refused_by(
        |fs| put(fs, STAGED, (NEW + 1, 2_000), NEW_HASH),
        (TARGET, FakeOp::Rename),
    );
    assert!(untouched_old(&fs));
    assert_eq!(size(&fs, STAGED), Some(NEW + 1));
}

#[test]
fn a_staged_file_rewritten_after_its_hash_is_refused() {
    let fs = refused_by(
        |fs| put(fs, STAGED, (NEW, 3_000), [5; 32]),
        (TARGET, FakeOp::Rename),
    );
    assert!(untouched_old(&fs));
}

fn untouched_old(fs: &FakeFs) -> bool {
    size(fs, TARGET) == Some(OLD) && hash_at(fs, TARGET) == OLD_HASH
}

#[test]
fn a_write_whose_hold_cannot_be_journalled_is_refused_before_anything_moves() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = Harness {
        log: Log {
            unjournaled: true,
            ..Log::default()
        },
        ..keeping(0)
    };
    let outcome = harness.run(&fs, &plan, &[]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::NotSupportedYet)));
    assert!(untouched(&fs, STAGED));
    assert_eq!(fs.calls(TARGET, FakeOp::Rename), 0);
}

#[test]
fn the_staged_file_and_both_folders_are_flushed_before_and_after_placing() {
    let seen_at_placing = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen_at_placing);
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = keeping(0);
    let ops = Crashing::meddling(&fs, (STAGED, FakeOp::Rename), move |fs| {
        let counts = [STAGED, "dest"].map(|path| fs.calls(path, FakeOp::Flush));
        record
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(counts);
    });
    assert_eq!(harness.run(&ops, &plan, &[]), JobOutcome::Done);
    let placing = seen_at_placing
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(placing, [[1, 1]]);
    assert_eq!(fs.calls("dest", FakeOp::Flush), 2);
    let holder = harness.log.held[0].held.parent().expect("holding folder");
    assert_eq!(fs.calls(&holder.path().to_text(), FakeOp::Flush), 1);
}

fn put_back_after_a_failed_placing(flush_fails: bool) -> (FakeFs, Harness) {
    let (fs, plan) = saving(STAGED, seen());
    let sharing = Fault::Fail(FsErrorKind::SharingViolation);
    fs.fault(STAGED, FakeOp::Rename, 1, sharing);
    if flush_fails {
        fs.fault("dest", FakeOp::Flush, 2, Fault::Fail(FsErrorKind::Unknown));
    }
    let mut harness = keeping(0);
    assert!(failure(&harness.run(&fs, &plan, &[])).is_some());
    assert!(untouched_old(&fs));
    assert!(harness.log.done().is_empty());
    (fs, harness)
}

#[test]
fn a_put_back_flushes_both_folders() {
    let (fs, harness) = put_back_after_a_failed_placing(false);
    let holder = harness.log.held[0].held.parent().expect("holding folder");
    assert_eq!(fs.calls("dest", FakeOp::Flush), 2);
    assert_eq!(fs.calls(&holder.path().to_text(), FakeOp::Flush), 2);
    assert!(harness.log.stranded.is_empty());
}

#[test]
fn a_put_back_whose_flush_fails_strands_the_hold() {
    let (_, harness) = put_back_after_a_failed_placing(true);
    assert_eq!(harness.log.stranded.len(), 1);
}

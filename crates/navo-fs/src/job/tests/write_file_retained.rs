use navo_core::FileStamp;

use super::crashing::Crashing;
use super::kept_aside::{entry_at, keeping};
use super::write_file::{
    NEW, NEW_HASH, OLD, OLD_HASH, STAGED, TARGET, hash_at, put, saving, seen, untouched,
};
use super::*;

pub(super) fn retaining(workers: usize) -> Harness {
    Harness {
        log: Log {
            retaining: true,
            ..Log::default()
        },
        ..keeping(workers)
    }
}

fn identity_at(fs: &FakeFs, path: &str) -> crate::FileIdentity {
    fs.file_identity(&fs.location(path))
        .expect("identity read")
        .expect("identity known")
}

fn refused_with(expect: FileStamp, kind: FsErrorKind) {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, expect);
        let mut harness = retaining(workers);
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(failure(&outcome), Some((0, kind)), "workers {workers}");
        assert!(untouched(&fs, STAGED), "workers {workers}");
        assert!(harness.log.proofs.is_empty(), "workers {workers}");
    }
}

#[test]
fn a_retained_save_journals_a_proof_matching_its_held_row() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let old = identity_at(&fs, TARGET);
        let new = identity_at(&fs, STAGED);
        let mut harness = retaining(workers);
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        let [(held, proof)] = harness.log.proofs.as_slice() else {
            panic!("one proof, workers {workers}");
        };
        assert_eq!(harness.log.held.as_slice(), std::slice::from_ref(held));
        assert_eq!(held.placed_hash, Some(NEW_HASH));
        let kept = held.held.path().to_text();
        let container = held.held.parent().expect("holder").path().to_text();
        assert_eq!(proof.node.identity, old, "workers {workers}");
        assert_eq!(
            (proof.node.size, proof.node.modified_ms),
            (OLD, Some(1_000))
        );
        assert_eq!(proof.replacement_identity, new);
        assert_eq!(identity_at(&fs, &kept), old);
        assert_eq!(identity_at(&fs, TARGET), new);
        assert_eq!(proof.parent_identity, identity_at(&fs, "dest"));
        assert_eq!(proof.container_identity, identity_at(&fs, &container));
        assert_eq!(fs.calls(&container, FakeOp::Secure), 1);
        assert_eq!(hash_at(&fs, &kept), OLD_HASH);
        assert_eq!(hash_at(&fs, TARGET), NEW_HASH);
        assert!(!exists(&fs, STAGED));
    }
}

#[test]
fn a_save_without_retention_journals_no_proof_and_hides_nothing() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = keeping(0);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    assert!(harness.log.proofs.is_empty());
    let container = harness.log.held[0].held.parent().expect("holder");
    assert_eq!(fs.calls(&container.path().to_text(), FakeOp::Secure), 0);
    assert_eq!(hash_at(&fs, TARGET), NEW_HASH);
}

#[test]
fn a_retained_save_refuses_a_changed_size_time_or_hash() {
    refused_with(
        FileStamp {
            size: OLD + 1,
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
    refused_with(
        FileStamp {
            modified_ms: Some(999),
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
    refused_with(
        FileStamp {
            hash: [7; 32],
            ..seen()
        },
        FsErrorKind::ChangedOnDisk,
    );
}

#[test]
fn a_retained_save_refuses_a_missing_target_and_a_wrong_staged_file() {
    let (fs, plan) = saving(STAGED, seen());
    fs.remove(TARGET);
    let mut harness = retaining(0);
    let outcome = harness.run(&fs, &plan, &[]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert_eq!(size(&fs, STAGED), Some(NEW));
    assert!(harness.log.proofs.is_empty());
    let (fs, plan) = saving(STAGED, seen());
    put(&fs, STAGED, (NEW + 1, 2_000), NEW_HASH);
    let mut harness = retaining(0);
    let outcome = harness.run(&fs, &plan, &[]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert_eq!(hash_at(&fs, TARGET), OLD_HASH);
    assert!(harness.log.proofs.is_empty());
}

#[test]
fn a_retained_save_refuses_a_staged_file_on_another_volume_or_mount() {
    let staged = "staging/a.txt.part";
    for volume in [true, false] {
        let (fs, plan) = saving(staged, seen());
        if volume {
            fs.set_volume("staging", 9);
        } else {
            fs.set_mount_identity("staging", Some(5));
        }
        let mut harness = retaining(0);
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(failure(&outcome), Some((0, FsErrorKind::NotSupportedYet)));
        assert!(untouched(&fs, staged));
        assert!(harness.log.proofs.is_empty());
    }
}

#[test]
fn a_retained_save_whose_proof_cannot_be_journalled_moves_nothing() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = Harness {
        log: Log {
            unjournaled: true,
            ..retaining(0).log
        },
        ..keeping(0)
    };
    assert!(failure(&harness.run(&fs, &plan, &[])).is_some());
    assert!(untouched(&fs, STAGED));
    assert_eq!(fs.calls(TARGET, FakeOp::Rename), 0);
    assert!(super::kept_aside::holding(&fs).is_empty());
    assert!(harness.log.proofs.is_empty());
}

fn refused_by(meddle: impl Fn(&FakeFs) + Send + Sync + 'static, at: (&str, FakeOp)) -> FakeFs {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = retaining(0);
    let ops = Crashing::meddling(&fs, at, meddle);
    let outcome = harness.run(&ops, &plan, &[]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::ChangedOnDisk)));
    assert!(harness.log.stranded.is_empty());
    assert!(!exists(&fs, &harness.log.held[0].held.path().to_text()));
    assert!(harness.log.done().is_empty());
    fs
}

#[test]
fn a_retained_save_puts_back_a_target_rewritten_after_its_hash() {
    let fs = refused_by(
        |fs| put(fs, TARGET, (OLD, 1_000), [8; 32]),
        (STAGED, FakeOp::Read),
    );
    assert_eq!(hash_at(&fs, TARGET), [8; 32]);
    assert_eq!(hash_at(&fs, STAGED), NEW_HASH);
}

#[test]
fn a_retained_save_puts_back_a_target_replaced_after_its_hash() {
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
fn a_retained_save_refuses_a_staged_file_rewritten_during_the_hold() {
    let fs = refused_by(
        |fs| put(fs, STAGED, (NEW, 3_000), [5; 32]),
        (TARGET, FakeOp::Rename),
    );
    assert_eq!(hash_at(&fs, TARGET), OLD_HASH);
}

#[test]
fn a_retained_save_refuses_a_staged_file_swapped_for_another_of_the_same_stamp() {
    let fs = refused_by(
        |fs| {
            fs.remove(STAGED);
            put(fs, STAGED, (NEW, 2_000), NEW_HASH);
        },
        (TARGET, FakeOp::Rename),
    );
    assert_eq!(hash_at(&fs, TARGET), OLD_HASH);
    assert_eq!(size(&fs, STAGED), Some(NEW));
}

#[test]
fn a_cancel_after_a_retained_hold_puts_the_old_file_back() {
    for workers in MODES {
        let (fs, plan) = saving(STAGED, seen());
        let mut harness = retaining(workers);
        let ops = Crashing::cancelling(&fs, TARGET, FakeOp::Rename, &harness.cancel);
        assert_eq!(harness.run(&ops, &plan, &[]), JobOutcome::Cancelled);
        assert_eq!(hash_at(&fs, TARGET), OLD_HASH, "workers {workers}");
        assert_eq!(size(&fs, STAGED), Some(NEW), "workers {workers}");
        assert!(harness.log.stranded.is_empty(), "workers {workers}");
    }
}

#[test]
fn a_failed_retained_placing_puts_the_old_file_back_and_flushes_both_folders() {
    let (fs, plan) = saving(STAGED, seen());
    fs.fault(
        STAGED,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::SharingViolation),
    );
    let mut harness = retaining(0);
    assert!(failure(&harness.run(&fs, &plan, &[])).is_some());
    assert_eq!(hash_at(&fs, TARGET), OLD_HASH);
    assert_eq!(
        entry_at(&fs, TARGET).and_then(|e| e.modified_ms),
        Some(1_000)
    );
    let holder = harness.log.held[0].held.parent().expect("holder");
    assert_eq!(fs.calls(&holder.path().to_text(), FakeOp::Flush), 2);
    assert_eq!(fs.calls("dest", FakeOp::Flush), 2);
    assert!(harness.log.stranded.is_empty());
}

#[test]
fn a_retained_put_back_onto_an_occupied_target_strands_the_hold_and_keeps_both() {
    let (fs, plan) = saving(STAGED, seen());
    let mut harness = retaining(0);
    let ops = Crashing::meddling(&fs, (STAGED, FakeOp::Rename), |fs| {
        put(fs, TARGET, (5, 7_000), [9; 32]);
    });
    assert!(failure(&harness.run(&ops, &plan, &[])).is_some());
    let [stranded] = harness.log.stranded.as_slice() else {
        panic!("the hold is stranded");
    };
    assert_eq!(hash_at(&fs, &stranded.held.path().to_text()), OLD_HASH);
    assert_eq!(hash_at(&fs, TARGET), [9; 32]);
    assert_eq!(hash_at(&fs, STAGED), NEW_HASH);
}

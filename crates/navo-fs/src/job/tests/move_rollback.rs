//! Crash rollback of a move whose source removal may have run but was never marked done: the copy
//! is the only copy unless the source path still holds the file the copy read.

use super::*;

const VOLUME: u64 = 7;
const OLD: u64 = 3;

fn moving() -> (FakeFs, OperationPlan) {
    let fs = fixture(&[("src/a.txt", BIG)]);
    fs.set_volume_serial("", Some(VOLUME));
    let plan = plan(&request(&fs, OpKind::Move, &["src/a.txt"], "dst"));
    (fs, plan)
}

fn removal_seq(plan: &OperationPlan) -> u32 {
    let at = plan
        .steps
        .iter()
        .position(|step| matches!(step, Step::RemoveFile { .. }))
        .expect("a cross-volume move removes its source");
    u32::try_from(at).expect("small plan")
}

/// Every step but the source removal: the job crashed before it marked the removal done.
fn crashed_before_marking_removal(plan: &OperationPlan) -> Vec<u32> {
    let removal = removal_seq(plan);
    all_seqs(plan)
        .into_iter()
        .filter(|seq| *seq != removal)
        .collect()
}

/// A different file with the original's name, size and time: only its file ID tells it apart.
fn lookalike(fs: &FakeFs) {
    fs.add_entry(
        "src",
        FsEntry {
            name: navo_core::ProviderPath::from_text("a.txt"),
            kind: EntryKind::File,
            size: BIG,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        },
    );
}

#[test]
fn a_rollback_keeps_the_copy_when_the_source_path_holds_another_file() {
    for workers in MODES {
        let (fs, plan) = moving();
        let mut harness = Harness {
            workers,
            ..Harness::default()
        };
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        assert!(
            !exists(&fs, "src/a.txt"),
            "workers {workers}: source removed"
        );
        lookalike(&fs);
        let done = crashed_before_marking_removal(&plan);
        let mut next_start = Harness {
            placed: harness.log.placed.clone(),
            ..Harness::default()
        };
        assert_eq!(next_start.roll_back(&fs, &plan, &done), JobOutcome::Done);
        assert!(
            exists(&fs, "dst/a.txt"),
            "workers {workers}: the rollback deleted the only copy of the moved file"
        );
    }
}

/// Runs `plan` up to its source removal: the copy is placed and journaled, the source is intact.
fn copied_only(fs: &FakeFs, plan: &OperationPlan, mut harness: Harness) -> Harness {
    let mut head = plan.clone();
    head.steps.truncate(removal_seq(plan) as usize);
    assert_eq!(harness.run(fs, &head, &[]), JobOutcome::Done);
    harness
}

fn done_before_removal(plan: &OperationPlan) -> Vec<u32> {
    (0..removal_seq(plan)).collect()
}

fn rolled_back(fs: &FakeFs, plan: &OperationPlan, ran: &Harness, done: &[u32]) -> Log {
    let mut next_start = Harness {
        held: ran.log.held.clone(),
        placed: ran.log.placed.clone(),
        ..Harness::default()
    };
    assert_eq!(next_start.roll_back(fs, plan, done), JobOutcome::Done);
    next_start.log
}

#[test]
fn a_move_journals_the_source_proof_with_its_placed_copy() {
    for workers in MODES {
        let (fs, plan) = moving();
        let (_, harness) = run_in(workers, &fs, &plan);
        let proofs: Vec<_> = harness.log.placed.iter().map(|file| file.source).collect();
        assert!(
            matches!(proofs.as_slice(), [Some(proof)] if proof.identity.is_some()),
            "workers {workers}: {proofs:?}"
        );
    }
}

#[test]
fn a_rollback_keeps_the_copy_when_the_source_is_gone() {
    let (fs, plan) = moving();
    let (_, harness) = run(&fs, &plan);
    let log = rolled_back(&fs, &plan, &harness, &crashed_before_marking_removal(&plan));
    assert_eq!(size(&fs, "dst/a.txt"), Some(BIG));
    assert!(
        log.copies.is_empty(),
        "the source is gone: the copy is no duplicate"
    );
}

#[test]
fn a_rollback_removes_the_copy_while_the_source_is_proved_unchanged() {
    for workers in MODES {
        let (fs, plan) = moving();
        let harness = copied_only(
            &fs,
            &plan,
            Harness {
                workers,
                ..Harness::default()
            },
        );
        let log = rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
        assert_eq!(
            (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
            (false, Some(BIG)),
            "workers {workers}"
        );
        assert!(log.copies.is_empty(), "workers {workers}: {:?}", log.copies);
    }
}

#[test]
fn an_old_journal_row_without_a_source_proof_keeps_the_copy() {
    let (fs, plan) = moving();
    let mut harness = copied_only(&fs, &plan, Harness::default());
    for file in &mut harness.log.placed {
        file.source = None;
    }
    let log = rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    let copy_seq = done_before_removal(&plan)
        .into_iter()
        .find(|seq| matches!(plan.steps[*seq as usize], Step::CopyFile { .. }));
    assert_eq!(
        (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
        (true, Some(BIG))
    );
    assert!(
        log.undone
            .contains(&(copy_seq.unwrap(), StepOutcome::Skipped))
    );
    assert_eq!(
        log.copies,
        [crate::KeptCopy {
            seq: copy_seq.unwrap(),
            location: fs.location("dst/a.txt"),
        }],
        "a kept copy is reported, not only logged"
    );
}

#[test]
fn a_rollback_over_a_held_file_keeps_the_copy_when_the_source_path_holds_another_file() {
    for workers in MODES {
        let (fs, plan) = super::move_over::moving_over(false);
        fs.set_volume_serial("", Some(VOLUME));
        let mut harness = super::kept_aside::keeping(workers);
        assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
        lookalike(&fs);
        let log = rolled_back(&fs, &plan, &harness, &crashed_before_marking_removal(&plan));
        assert_eq!(
            size(&fs, "dst/a.txt"),
            Some(BIG),
            "workers {workers}: copy lost"
        );
        let beside: Vec<u64> = log
            .beside
            .iter()
            .filter_map(|at| fs.metadata(at).ok().map(|entry| entry.size))
            .collect();
        assert_eq!(beside, vec![OLD], "workers {workers}: old file kept beside");
    }
}

#[test]
fn a_rollback_over_a_held_file_restores_it_while_the_source_is_proved_unchanged() {
    let (fs, plan) = super::move_over::moving_over(false);
    fs.set_volume_serial("", Some(VOLUME));
    let harness = copied_only(&fs, &plan, super::kept_aside::keeping(0));
    rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert_eq!(
        (size(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
        (Some(OLD), Some(BIG))
    );
}

#[test]
fn a_rollback_keeps_the_copy_when_its_proof_has_no_file_id() {
    let (fs, plan) = moving();
    let mut harness = copied_only(&fs, &plan, Harness::default());
    fs.set_volume_serial("", None);
    for file in &mut harness.log.placed {
        let proof = file
            .source
            .as_mut()
            .expect("a move journals its source proof");
        proof.identity = None;
        proof.created_ms = Some(500);
        proof.attributes = Some(0x20);
        assert!(proof.proves(), "times and attributes prove the source");
    }
    rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert_eq!(
        (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
        (true, Some(BIG)),
        "without a file ID a path read cannot prove the source, so the copy stays (D164)"
    );
}

/// Journals a stale proof row and the proved one for the same copy, `stale_last` deciding order.
fn two_proof_rows(stale_last: bool) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = moving();
    let mut harness = copied_only(&fs, &plan, Harness::default());
    let proved = harness.log.placed.pop().expect("one placed copy");
    let mut stale = proved.clone();
    let proof = stale
        .source
        .as_mut()
        .expect("a move journals its source proof");
    let identity = proof.identity.as_mut().expect("the volume keeps file IDs");
    identity.file[0] ^= 1;
    let rows = if stale_last {
        [proved, stale]
    } else {
        [stale, proved]
    };
    harness.log.placed.extend(rows);
    (fs, plan, harness)
}

#[test]
fn the_newest_proof_row_for_a_copy_decides_its_rollback() {
    for stale_last in [true, false] {
        let (fs, plan, harness) = two_proof_rows(stale_last);
        rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
        assert_eq!(
            (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
            (stale_last, Some(BIG)),
            "stale proof last: {stale_last}"
        );
    }
}

/// A copied-only move whose proof has no file ID yet matches the source by path on size, times
/// and attributes, as the Windows read of a FAT volume does.
fn proved_without_id() -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = moving();
    let mut harness = copied_only(&fs, &plan, Harness::default());
    fs.set_without_ids("src");
    fs.set_path_details(500, 0x20);
    for file in &mut harness.log.placed {
        let proof = file
            .source
            .as_mut()
            .expect("a move journals its source proof");
        proof.identity = None;
        proof.created_ms = Some(500);
        proof.attributes = Some(0x20);
    }
    (fs, plan, harness)
}

fn kept_copy_of(fs: &FakeFs, plan: &OperationPlan) -> crate::KeptCopy {
    let seq = done_before_removal(plan)
        .into_iter()
        .find(|seq| matches!(plan.steps[*seq as usize], Step::CopyFile { .. }))
        .expect("a move copies its file");
    crate::KeptCopy {
        seq,
        location: fs.location("dst/a.txt"),
    }
}

#[test]
fn a_no_id_proof_removes_the_copy_when_source_and_copy_hold_the_same_bytes() {
    let (fs, plan, harness) = proved_without_id();
    let log = rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert_eq!(
        (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
        (false, Some(BIG))
    );
    assert!(log.copies.is_empty(), "{:?}", log.copies);
}

#[test]
fn a_no_id_proof_keeps_the_copy_when_the_source_holds_other_bytes() {
    let (fs, plan, harness) = proved_without_id();
    fs.set_hash("src/a.txt", [7; 32]);
    let log = rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert!(
        exists(&fs, "dst/a.txt"),
        "a tunneled lookalike with other bytes deleted the only copy (D164)"
    );
    assert_eq!(log.copies, [kept_copy_of(&fs, &plan)]);
}

#[test]
fn a_no_id_proof_keeps_the_copy_when_either_file_cannot_be_read() {
    for path in ["src/a.txt", "dst/a.txt"] {
        let (fs, plan, harness) = proved_without_id();
        let next = fs.calls(path, FakeOp::Read) + 1;
        fs.fault(
            path,
            FakeOp::Read,
            next,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
        let log = rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
        assert!(exists(&fs, "dst/a.txt"), "{path}: copy removed unread");
        assert_eq!(log.copies, [kept_copy_of(&fs, &plan)], "{path}");
    }
}

#[test]
fn a_proof_with_a_file_id_reads_no_content_before_removing_the_copy() {
    let (fs, plan) = moving();
    let harness = copied_only(&fs, &plan, Harness::default());
    let reads =
        |fs: &FakeFs| fs.calls("src/a.txt", FakeOp::Read) + fs.calls("dst/a.txt", FakeOp::Read);
    let before = reads(&fs);
    rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert!(!exists(&fs, "dst/a.txt"));
    assert_eq!(reads(&fs), before, "a file ID needs no content compare");
}

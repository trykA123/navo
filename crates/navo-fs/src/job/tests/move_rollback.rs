//! Crash rollback of a move whose source removal may have run but was never marked done: the copy
//! is the only copy unless the source path still holds the file the copy read.

use super::*;

const VOLUME: u64 = 7;
const OLD: u64 = 3;

fn moving() -> (FakeFs, OperationPlan) {
    moving_all(&["src/a.txt"])
}

fn moving_all(paths: &[&str]) -> (FakeFs, OperationPlan) {
    let files: Vec<_> = paths.iter().map(|path| (*path, BIG)).collect();
    let fs = fixture(&files);
    fs.set_volume_serial("", Some(VOLUME));
    let plan = plan(&request(&fs, OpKind::Move, paths, "dst"));
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
        cancel: ran.cancel.clone(),
        held: ran.log.held.clone(),
        placed: ran.log.placed.clone(),
        pending: ran.pending.clone(),
        ..Harness::default()
    };
    assert_eq!(next_start.roll_back(fs, plan, done), JobOutcome::Done);
    next_start.log
}

/// The steps startup recovery reads as pending: every step not in `done`.
fn pending_besides(plan: &OperationPlan, done: &[u32]) -> Vec<u32> {
    all_seqs(plan)
        .into_iter()
        .filter(|seq| !done.contains(seq))
        .collect()
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

/// The bytes every moved file and its copy hold, unless a test changes the source.
const SAME: [u8; 32] = [9; 32];

/// A copied-only move whose proofs have no file ID yet match their sources by path on size, times
/// and attributes, as the Windows read of a FAT volume does.
fn proved_without_id(paths: &[&str]) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = moving_all(paths);
    for path in paths {
        fs.set_hash(path, SAME);
    }
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

/// A no-ID proof whose source removal may have run unmarked: every step before it is done.
fn proved_while_removal_unsure() -> (FakeFs, OperationPlan, Harness, Vec<u32>) {
    let (fs, plan, mut harness) = proved_without_id(&["src/a.txt"]);
    let done = done_before_removal(&plan);
    harness.pending = Some(pending_besides(&plan, &done));
    (fs, plan, harness, done)
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

fn copy_seq(fs: &FakeFs, plan: &OperationPlan, target: &str) -> u32 {
    let target = fs.location(target);
    let at = plan
        .steps
        .iter()
        .position(
            |step| matches!(step, Step::CopyFile { target: to, .. } if to.same_place(&target)),
        )
        .expect("the move copies this file");
    u32::try_from(at).expect("small plan")
}

fn content_reads(fs: &FakeFs) -> u32 {
    fs.calls("src/a.txt", FakeOp::Read) + fs.calls("dst/a.txt", FakeOp::Read)
}

#[test]
fn a_no_id_proof_reads_no_content_while_the_source_removal_never_started() {
    let (fs, plan, mut harness) = proved_without_id(&["src/a.txt", "src/b.txt"]);
    let in_flight = copy_seq(&fs, &plan, "dst/b.txt");
    assert!(
        in_flight < removal_seq(&plan),
        "a move copies before it removes"
    );
    let done: Vec<u32> = done_before_removal(&plan)
        .into_iter()
        .filter(|seq| *seq != in_flight)
        .collect();
    harness.pending = Some(pending_besides(&plan, &done));
    let before = content_reads(&fs);
    let log = rolled_back(&fs, &plan, &harness, &done);
    assert!(!exists(&fs, "dst/a.txt"), "the duplicate copy stayed");
    assert_eq!(
        content_reads(&fs),
        before,
        "a copy still in flight proves no removal started: no byte compare (D165)"
    );
    assert!(log.copies.is_empty(), "{:?}", log.copies);
}

#[test]
fn an_unsure_removal_removes_the_copy_when_source_and_copy_hold_the_same_bytes() {
    let (fs, plan, harness, done) = proved_while_removal_unsure();
    let before = content_reads(&fs);
    let log = rolled_back(&fs, &plan, &harness, &done);
    assert_eq!(
        (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
        (false, Some(BIG))
    );
    assert!(
        content_reads(&fs) > before,
        "the copy was removed uncompared"
    );
    assert!(log.copies.is_empty(), "{:?}", log.copies);
}

#[test]
fn an_unsure_removal_keeps_the_copy_when_the_source_holds_other_bytes() {
    let (fs, plan, harness, done) = proved_while_removal_unsure();
    fs.set_hash("src/a.txt", [7; 32]);
    let log = rolled_back(&fs, &plan, &harness, &done);
    assert!(
        exists(&fs, "dst/a.txt"),
        "a tunneled lookalike with other bytes deleted the only copy (D164)"
    );
    assert_eq!(log.copies, [kept_copy_of(&fs, &plan)]);
}

#[test]
fn a_cancel_reaching_the_byte_compare_keeps_the_copy() {
    let (fs, plan, harness, done) = proved_while_removal_unsure();
    harness.cancel.cancel();
    let log = rolled_back(&fs, &plan, &harness, &done);
    assert!(
        exists(&fs, "dst/a.txt"),
        "a cancelled compare removed the copy"
    );
    assert_eq!(log.copies, [kept_copy_of(&fs, &plan)]);
}

#[test]
fn a_no_id_proof_keeps_the_copy_when_either_file_cannot_be_read() {
    for path in ["src/a.txt", "dst/a.txt"] {
        let (fs, plan, harness, done) = proved_while_removal_unsure();
        let next = fs.calls(path, FakeOp::Read) + 1;
        fs.fault(
            path,
            FakeOp::Read,
            next,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
        let log = rolled_back(&fs, &plan, &harness, &done);
        assert!(exists(&fs, "dst/a.txt"), "{path}: copy removed unread");
        assert_eq!(log.copies, [kept_copy_of(&fs, &plan)], "{path}");
    }
}

#[test]
fn a_proof_with_a_file_id_reads_no_content_before_removing_the_copy() {
    let (fs, plan) = moving();
    let harness = copied_only(&fs, &plan, Harness::default());
    let before = content_reads(&fs);
    rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
    assert!(!exists(&fs, "dst/a.txt"));
    assert_eq!(
        content_reads(&fs),
        before,
        "a file ID needs no content compare"
    );
}

/// Files in the scale test: enough that a per-copy scan of the plan would show.
const MANY: usize = 1_000;

#[test]
fn a_copy_phase_crash_over_many_no_id_files_checks_each_copy_without_scanning_the_plan() {
    let names: Vec<String> = (0..MANY).map(|at| format!("src/f{at}.txt")).collect();
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let (fs, plan, mut harness) = proved_without_id(&paths);
    let in_flight = copy_seq(&fs, &plan, &format!("dst/f{}.txt", MANY - 1));
    let done: Vec<u32> = done_before_removal(&plan)
        .into_iter()
        .filter(|seq| *seq != in_flight)
        .collect();
    harness.pending = Some(pending_besides(&plan, &done));
    let reads = |fs: &FakeFs| -> u32 {
        paths
            .iter()
            .map(|path| {
                let copy = path.replacen("src/", "dst/", 1);
                fs.calls(path, FakeOp::Read) + fs.calls(&copy, FakeOp::Read)
            })
            .sum()
    };
    let before = reads(&fs);
    super::super::undo::STEPS_SCANNED.with(|count| count.set(0));
    let log = rolled_back(&fs, &plan, &harness, &done);
    let scanned = super::super::undo::STEPS_SCANNED.with(std::cell::Cell::get);
    assert_eq!(scanned, 0, "the D165 check scanned plan steps per copy");
    assert_eq!(reads(&fs), before, "a copy-phase crash compares no bytes");
    assert!(log.copies.is_empty(), "{:?}", log.copies);
    assert!(!exists(&fs, "dst/f0.txt"), "the duplicate copy stayed");
}

/// Files in the rollback cost test: a per-copy scan of the plan or the journal costs ~10^9 here.
const HUGE: usize = 20_000;

#[test]
fn a_removal_phase_crash_over_huge_moves_rolls_back_in_linear_compares() {
    let names: Vec<String> = (0..HUGE).map(|at| format!("src/f{at}.txt")).collect();
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let (fs, plan, mut harness) = proved_without_id(&paths);
    let removals: Vec<u32> = (removal_seq(&plan)..)
        .zip(&plan.steps[removal_seq(&plan) as usize..])
        .filter(|(_, step)| matches!(step, Step::RemoveFile { .. }))
        .map(|(seq, _)| seq)
        .collect();
    assert_eq!(removals.len(), HUGE, "one removal per moved file");
    let (ran, unsure) = removals.split_at(HUGE / 2);
    let mut done = done_before_removal(&plan);
    done.extend(ran);
    harness.pending = Some(unsure.to_vec());
    super::super::undo::COMPARED.with(|count| count.set(0));
    let log = rolled_back(&fs, &plan, &harness, &done);
    let compared = super::super::undo::COMPARED.with(std::cell::Cell::get);
    let bound = 8 * plan.steps.len() as u64;
    assert!(
        compared <= bound,
        "the rollback compared {compared} times, over {bound}"
    );
    assert!(log.copies.is_empty(), "{:?}", log.copies.len());
    let copy_of = |removal: u32| {
        let Step::RemoveFile { location } = &plan.steps[removal as usize] else {
            unreachable!("a removal seq");
        };
        plan.steps.iter().find_map(|step| match step {
            Step::CopyFile { source, target, .. } if source == location => Some(target.clone()),
            _ => None,
        })
    };
    let kept = copy_of(ran[0]).expect("a removed source was copied");
    assert!(
        fs.metadata(&kept).is_ok(),
        "a removed source's copy was removed"
    );
    let duplicate = copy_of(unsure[0]).expect("an unsure source was copied");
    assert!(
        fs.metadata(&duplicate).is_err(),
        "an unsure removal's duplicate copy stayed"
    );
}

#[test]
fn the_unmarked_removal_window_runs_from_the_first_removal_to_the_first_pending_other_step() {
    let at =
        |path: &str| navo_core::Location::local(navo_core::ProviderPath::from_text(path)).unwrap();
    let copy = |name: &str| Step::CopyFile {
        source: at(&format!("/src/{name}")),
        target: at(&format!("/dst/{name}")),
        size: 1,
    };
    let remove = |name: &str| Step::RemoveFile {
        location: at(&format!("/src/{name}")),
    };
    let steps = vec![
        Step::MakeFolder { target: at("/dst") },
        copy("a"),
        remove("a"),
        copy("b"),
        remove("b"),
    ];
    let plan = OperationPlan {
        kind: OpKind::Move,
        steps,
        totals: navo_core::Totals {
            files: 2,
            folders: 1,
            bytes: 2,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    assert_eq!(
        unmarked_removals(&plan, [3, 4]),
        2..3,
        "first removal in, copy b out"
    );
    assert_eq!(
        unmarked_removals(&plan, [4]),
        2..u32::MAX,
        "a pending removal ends nothing"
    );
    assert_eq!(
        unmarked_removals(&plan, []),
        2..u32::MAX,
        "nothing pending: to the plan end"
    );
    assert!(
        unmarked_removals(&plan, [1, 2, 3, 4]).is_empty(),
        "no removal started"
    );
    let mut copies = plan.clone();
    copies
        .steps
        .retain(|step| !matches!(step, Step::RemoveFile { .. }));
    assert!(
        unmarked_removals(&copies, [2]).is_empty(),
        "no removal step"
    );
    assert!(
        unmarked_removals(&copies, []).is_empty(),
        "no removal step, nothing pending"
    );
}

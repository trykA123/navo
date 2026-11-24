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
    rolled_back(&fs, &plan, &harness, &crashed_before_marking_removal(&plan));
    assert_eq!(size(&fs, "dst/a.txt"), Some(BIG));
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
        rolled_back(&fs, &plan, &harness, &done_before_removal(&plan));
        assert_eq!(
            (exists(&fs, "dst/a.txt"), size(&fs, "src/a.txt")),
            (false, Some(BIG)),
            "workers {workers}"
        );
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

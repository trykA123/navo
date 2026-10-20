use super::*;

fn copied(workers: usize) -> (FakeFs, OperationPlan, Harness) {
    let fs = fixture(&[("src/a.txt", BIG)]);
    let plan = copy_plan(&fs, &["src/a.txt"], "dst");
    let mut harness = Harness {
        workers,
        ..Harness::default()
    };
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undo(
    fs: &FakeFs,
    plan: &OperationPlan,
    placed: &[PlacedFile],
    unreadable: bool,
) -> (JobOutcome, Log) {
    let mut harness = Harness::default();
    let done = all_seqs(plan);
    let job = Journaled {
        plan,
        done: &done,
        held: &[],
        placed,
        held_unreadable: unreadable,
        unsure: None,
    };
    let outcome = harness.with(fs, |ctx| roll_back(&job, ctx));
    (outcome, harness.log)
}

fn stamp(modified_ms: Option<i64>) -> Vec<PlacedFile> {
    vec![PlacedFile {
        target: FakeFs::new().location("dst/a.txt"),
        stamp: FileStamp {
            size: BIG,
            modified_ms,
        },
    }]
}

#[test]
fn a_new_copy_journals_the_size_and_time_it_was_placed_with() {
    for workers in MODES {
        let (fs, _, harness) = copied(workers);
        assert_eq!(harness.log.placed, stamp(Some(1_000)), "workers {workers}");
        assert_eq!(size(&fs, "dst/a.txt"), Some(BIG));
    }
}

#[test]
fn a_copy_that_still_matches_its_stamp_is_removed() {
    let (fs, plan, _) = copied(0);
    let (outcome, _) = undo(&fs, &plan, &stamp(Some(1_000)), false);
    assert_eq!(
        (outcome, exists(&fs, "dst/a.txt")),
        (JobOutcome::Done, false)
    );
}

#[test]
fn a_copy_without_a_journaled_stamp_is_kept() {
    let (fs, plan, _) = copied(0);
    let (outcome, log) = undo(&fs, &plan, &[], false);
    assert_eq!(
        (outcome, exists(&fs, "dst/a.txt"), log.entries),
        (JobOutcome::Done, true, vec![(0, StepOutcome::Skipped)])
    );
}

#[test]
fn a_copy_whose_time_differs_even_slightly_is_kept() {
    let (fs, plan, _) = copied(0);
    let (_, _) = undo(&fs, &plan, &stamp(Some(1_001)), false);
    assert!(exists(&fs, "dst/a.txt"));
}

#[test]
fn a_stamp_without_a_modified_time_is_never_a_match() {
    let (fs, plan, _) = copied(0);
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", BIG);
    let (_, _) = undo(&fs, &plan, &stamp(None), false);
    assert!(exists(&fs, "dst/a.txt"));
}

#[test]
fn an_unreadable_held_row_leaves_a_copy_without_a_held_record_for_the_next_start() {
    let (fs, plan, _) = copied(0);
    let (outcome, log) = undo(&fs, &plan, &stamp(Some(1_000)), true);
    let copy_seq = plan
        .steps
        .iter()
        .position(|step| matches!(step, Step::CopyFile { .. }))
        .and_then(|seq| u32::try_from(seq).ok())
        .unwrap();
    assert_eq!(
        (
            outcome,
            exists(&fs, "dst/a.txt"),
            log.entries.iter().any(|(seq, _)| *seq == copy_seq)
        ),
        (JobOutcome::Done, true, false)
    );
}

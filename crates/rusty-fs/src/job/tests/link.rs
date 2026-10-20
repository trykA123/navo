use super::crashing::Crashing;
use super::*;

fn link_plan(fs: &FakeFs) -> OperationPlan {
    plan(&request(
        fs,
        OpKind::Link,
        &["src/a.txt", "src/b.txt"],
        "dst",
    ))
}

fn sources() -> FakeFs {
    fixture(&[("src/a.txt", 8), ("src/b.txt", 5)])
}

fn kind_of(fs: &FakeFs, path: &str) -> Option<EntryKind> {
    fs.metadata(&fs.location(path)).ok().map(|entry| entry.kind)
}

fn recover(
    fs: &dyn ProviderOps,
    plan: &OperationPlan,
    unsure: Option<u32>,
) -> (JobOutcome, Harness) {
    let mut harness = Harness::default();
    let job = Journaled {
        plan,
        done: &[],
        held: &[],
        placed: &[],
        held_unreadable: false,
        unsure,
    };
    let outcome = harness.with(fs, |ctx| roll_back(&job, ctx));
    (outcome, harness)
}

#[test]
fn makes_one_link_to_each_source() {
    for workers in MODES {
        let fs = sources();
        let (outcome, harness) = run_in(workers, &fs, &link_plan(&fs));
        assert_eq!(outcome, JobOutcome::Done);
        let mut done = harness.log.done();
        done.sort_unstable();
        assert_eq!(done, [0, 1]);
        for name in ["a.txt", "b.txt"] {
            let at = format!("dst/{name}");
            assert_eq!(kind_of(&fs, &at), Some(EntryKind::Link));
            assert_eq!(fs.link_target(&at), Some(format!("src/{name}")));
        }
        assert_eq!(
            (size(&fs, "src/a.txt"), size(&fs, "src/b.txt")),
            (Some(8), Some(5))
        );
        let last = harness.progress.last().unwrap();
        assert_eq!((last.files_done, last.files_total), (2, 2));
    }
}

#[test]
fn undo_removes_only_the_links_the_job_made() {
    let fs = sources();
    let before = tree_of(&fs, "");
    let plan = link_plan(&fs);
    let (_, mut harness) = run(&fs, &plan);
    fs.add_file("dst/c.txt", 3);
    let outcome = harness.roll_back(&fs, &plan, &[0, 1]);
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(
        harness.log.undone,
        [(1, StepOutcome::Done), (0, StepOutcome::Done)]
    );
    fs.remove("dst/c.txt");
    assert_eq!(tree_of(&fs, ""), before);
}

#[test]
fn a_name_taken_at_run_time_fails_and_keeps_the_existing_file() {
    for workers in MODES {
        let fs = sources();
        let plan = link_plan(&fs);
        fs.add_file("dst/a.txt", 3);
        let (outcome, mut harness) = run_in(workers, &fs, &plan);
        assert_eq!(failure(&outcome).map(|(seq, _)| seq), Some(0));
        assert!(harness.logged().contains(&(0, StepOutcome::Failed)));
        assert_eq!(kind_of(&fs, "dst/a.txt"), Some(EntryKind::File));
        let done = harness.log.done();
        assert_eq!(harness.roll_back(&fs, &plan, &done), JobOutcome::Done);
        assert_eq!(size(&fs, "dst/a.txt"), Some(3));
        assert_eq!(kind_of(&fs, "dst/b.txt"), None);
    }
}

#[test]
fn a_link_that_took_the_name_first_is_not_overwritten_or_undone() {
    let fs = sources();
    let plan = link_plan(&fs);
    add_link(&fs, "dst", "a.txt");
    let (outcome, mut harness) = run(&fs, &plan);
    assert_eq!(failure(&outcome).map(|(seq, _)| seq), Some(0));
    assert_eq!(fs.link_target("dst/a.txt"), None);
    assert_eq!(harness.roll_back(&fs, &plan, &[]), JobOutcome::Done);
    assert_eq!(kind_of(&fs, "dst/a.txt"), Some(EntryKind::Link));
}

#[test]
fn undo_keeps_a_file_that_replaced_the_link() {
    let fs = sources();
    let plan = link_plan(&fs);
    let (_, mut harness) = run(&fs, &plan);
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", 3);
    assert_eq!(harness.roll_back(&fs, &plan, &[0, 1]), JobOutcome::Done);
    assert_eq!(
        harness.log.undone,
        [(1, StepOutcome::Done), (0, StepOutcome::Skipped)]
    );
    assert_eq!(size(&fs, "dst/a.txt"), Some(3));
    assert_eq!(kind_of(&fs, "dst/b.txt"), None);
}

#[test]
fn undo_keeps_a_folder_that_replaced_the_link() {
    let fs = sources();
    let plan = link_plan(&fs);
    let (_, mut harness) = run(&fs, &plan);
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt/inside.txt", 4);
    assert_eq!(harness.roll_back(&fs, &plan, &[0, 1]), JobOutcome::Done);
    assert_eq!(harness.log.undone[1], (0, StepOutcome::Skipped));
    assert_eq!(size(&fs, "dst/a.txt/inside.txt"), Some(4));
}

#[test]
fn undo_of_a_link_already_gone_is_skipped() {
    let fs = sources();
    let plan = link_plan(&fs);
    let (_, mut harness) = run(&fs, &plan);
    fs.remove("dst/a.txt");
    assert_eq!(harness.roll_back(&fs, &plan, &[0, 1]), JobOutcome::Done);
    assert_eq!(harness.log.undone[1], (0, StepOutcome::Skipped));
}

#[test]
fn a_failed_link_removal_is_reported_and_keeps_the_link() {
    let fs = sources();
    let plan = link_plan(&fs);
    let (_, mut harness) = run(&fs, &plan);
    fs.fault(
        "dst/a.txt",
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let outcome = harness.roll_back(&fs, &plan, &[0, 1]);
    assert_eq!(failure(&outcome), Some((0, FsErrorKind::AccessDenied)));
    assert_eq!(
        harness.log.undone,
        [(1, StepOutcome::Done), (0, StepOutcome::Failed)]
    );
    assert_eq!(fs.link_target("dst/a.txt").as_deref(), Some("src/a.txt"));
}

#[test]
fn a_failure_on_the_second_link_rolls_back_to_no_links() {
    let fs = sources();
    let before = tree_of(&fs, "");
    let plan = link_plan(&fs);
    fs.fault(
        "dst/b.txt",
        FakeOp::Create,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let (outcome, mut harness) = run(&fs, &plan);
    assert_eq!(failure(&outcome), Some((1, FsErrorKind::AccessDenied)));
    let done = harness.log.done();
    assert_eq!(harness.roll_back(&fs, &plan, &done), JobOutcome::Done);
    assert_eq!(tree_of(&fs, ""), before);
}

#[test]
fn a_crash_between_the_link_and_its_journal_write_is_undone_by_recovery() {
    let fs = sources();
    let before = tree_of(&fs, "");
    let plan = link_plan(&fs);
    let mut harness = Harness::default();
    harness.log.fail_on = Some(0);
    let outcome = harness.run(&fs, &plan, &[]);
    assert_eq!(failure(&outcome).map(|(seq, _)| seq), Some(0));
    assert_eq!(kind_of(&fs, "dst/a.txt"), Some(EntryKind::Link));
    let (outcome, harness) = recover(&fs, &plan, Some(0));
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(harness.log.undone, [(0, StepOutcome::Done)]);
    assert_eq!(tree_of(&fs, ""), before);
}

#[test]
fn recovery_of_an_unsure_link_never_made_changes_nothing() {
    let fs = sources();
    let before = tree_of(&fs, "");
    let plan = link_plan(&fs);
    let crashing = Crashing::new(&fs, "dst/a.txt", FakeOp::Create);
    let _ = Harness::default().run(&crashing, &plan, &[]);
    assert_eq!(tree_of(&fs, ""), before);
    let (outcome, harness) = recover(&fs, &plan, Some(0));
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(harness.log.undone, [(0, StepOutcome::Skipped)]);
    assert_eq!(tree_of(&fs, ""), before);
}

#[test]
fn recovery_keeps_a_regular_file_at_an_unsure_link() {
    let fs = sources();
    let plan = link_plan(&fs);
    fs.add_file("dst/a.txt", 3);
    let (outcome, harness) = recover(&fs, &plan, Some(0));
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(harness.log.undone, [(0, StepOutcome::Skipped)]);
    assert_eq!(size(&fs, "dst/a.txt"), Some(3));
}

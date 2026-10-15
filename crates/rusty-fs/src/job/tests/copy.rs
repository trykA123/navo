use super::*;

fn one_file() -> (FakeFs, OperationPlan) {
    let fs = fixture(&[("src/one.txt", BIG)]);
    let plan = copy_plan(&fs, &["src/one.txt"], "dst");
    (fs, plan)
}

fn three_copied() -> (FakeFs, OperationPlan, JobOutcome, Harness) {
    let fs = three_files();
    let plan = copy_three(&fs);
    let (outcome, harness) = run(&fs, &plan);
    (fs, plan, outcome, harness)
}

fn bytes_seen(harness: &Harness) -> Vec<u64> {
    harness.progress.iter().map(|p| p.bytes_done).collect()
}

fn current_of(step: &Step) -> Option<rusty_core::Location> {
    match step {
        Step::MakeFolder { target } => Some(target.clone()),
        Step::CopyFile { source, .. } | Step::Rename { source, .. } => Some(source.clone()),
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => Some(location.clone()),
    }
}

#[test]
fn a_file_is_copied() {
    let (fs, plan) = one_file();
    let (outcome, _) = run(&fs, &plan);
    assert_eq!(
        (outcome, size(&fs, "dst/one.txt")),
        (JobOutcome::Done, Some(BIG))
    );
}

#[test]
fn a_copy_keeps_the_source() {
    let (fs, plan) = one_file();
    let _ = run(&fs, &plan);
    assert_eq!(size(&fs, "src/one.txt"), Some(BIG));
}

#[test]
fn a_copy_leaves_no_temporary_file() {
    let (fs, _, _, _) = three_copied();
    assert_eq!(leftovers(&fs), Vec::<String>::new());
}

#[test]
fn every_step_is_logged_done_in_order() {
    let (_, _, _, harness) = three_copied();
    assert_eq!(
        harness.logged(),
        [
            (0, StepOutcome::Done),
            (1, StepOutcome::Done),
            (2, StepOutcome::Done)
        ]
    );
}

#[test]
fn progress_bytes_never_go_down() {
    let (_, _, _, harness) = three_copied();
    let bytes = bytes_seen(&harness);
    assert!(bytes.windows(2).all(|pair| pair[0] <= pair[1]), "{bytes:?}");
}

#[test]
fn progress_reports_bytes_during_each_copy() {
    let (_, _, _, harness) = three_copied();
    let bytes = bytes_seen(&harness);
    assert!(
        [4096, 8192, BIG + 4096].iter().all(|b| bytes.contains(b)),
        "{bytes:?}"
    );
}

#[test]
fn the_last_progress_is_the_totals() {
    let (fs, plan, _, harness) = three_copied();
    assert_eq!(
        harness.progress.last(),
        Some(&Progress {
            bytes_done: plan.totals.bytes,
            bytes_total: plan.totals.bytes,
            files_done: plan.totals.files,
            files_total: plan.totals.files,
            current: Some(fs.location("src/three.txt")),
        })
    );
}

#[test]
fn every_progress_carries_the_plan_totals() {
    let (_, plan, _, harness) = three_copied();
    let totals: Vec<_> = harness
        .progress
        .iter()
        .map(|p| (p.bytes_total, p.files_total))
        .collect();
    assert_eq!(
        totals,
        vec![(19_000, 3); harness.progress.len()],
        "{:?}",
        plan.totals
    );
}

fn tree_fixture() -> (FakeFs, OperationPlan) {
    let fs = fixture(&[("src/b/c/d.txt", 5_000), ("src/b/e.txt", 7_000)]);
    let plan = copy_plan(&fs, &["src/b"], "dst");
    (fs, plan)
}

#[test]
fn a_folder_tree_is_copied() {
    let (fs, plan) = tree_fixture();
    let _ = run(&fs, &plan);
    assert_eq!(
        tree_of(&fs, "dst"),
        [
            entry("b", EntryKind::Folder, 0),
            entry("b/c", EntryKind::Folder, 0),
            entry("b/c/d.txt", EntryKind::File, 5_000),
            entry("b/e.txt", EntryKind::File, 7_000),
        ]
    );
}

#[test]
fn a_folder_tree_copy_logs_every_step_done() {
    let (fs, plan) = tree_fixture();
    let (_, harness) = run(&fs, &plan);
    let expected: Vec<_> = all_seqs(&plan)
        .into_iter()
        .map(|seq| (seq, StepOutcome::Done))
        .collect();
    assert_eq!(harness.logged(), expected);
}

#[test]
fn progress_is_reported_for_every_step() {
    let (fs, plan) = tree_fixture();
    let (_, harness) = run(&fs, &plan);
    let missing: Vec<_> = plan
        .steps
        .iter()
        .map(current_of)
        .filter(|current| !harness.progress.iter().any(|p| &p.current == current))
        .collect();
    assert_eq!(missing, Vec::new());
}

#[test]
fn a_folder_made_after_planning_is_merged() {
    let (fs, plan) = tree_fixture();
    fs.add_folder("dst/b");
    let (outcome, harness) = run(&fs, &plan);
    assert_eq!(
        (outcome, harness.logged().first().copied()),
        (JobOutcome::Done, Some((0, StepOutcome::Done)))
    );
}

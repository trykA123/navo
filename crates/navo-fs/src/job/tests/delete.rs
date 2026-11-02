use super::*;

fn delete_three(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        target: None,
        existing: Vec::new(),
        ..request(
            fs,
            OpKind::Delete,
            &["src/one.txt", "src/two.txt", "src/three.txt"],
            "dst",
        )
    })
}

fn bytes_and_files(harness: &Harness) -> Option<(u64, u64, u64, u64)> {
    harness
        .progress
        .last()
        .map(|p| (p.bytes_done, p.bytes_total, p.files_done, p.files_total))
}

#[test]
fn a_delete_counts_files_not_bytes() {
    let fs = three_files();
    let plan = delete_three(&fs);
    let (outcome, harness) = run(&fs, &plan);
    assert_eq!(
        (outcome, bytes_and_files(&harness)),
        (JobOutcome::Done, Some((0, 0, 3, 3)))
    );
}

#[test]
fn a_parallel_delete_counts_files_not_bytes() {
    let fs = three_files();
    let plan = delete_three(&fs);
    let (outcome, harness) = run_in(4, &fs, &plan);
    assert_eq!(
        (outcome, bytes_and_files(&harness)),
        (JobOutcome::Done, Some((0, 0, 3, 3)))
    );
}

use super::*;

#[test]
fn create_writes_job_row_and_step_rows() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 3, 100).unwrap();
    assert_eq!(count(&journal, "SELECT count(*) FROM job"), 1);
    let seqs: Vec<u32> = step_rows(&journal, job).into_iter().map(|r| r.0).collect();
    assert_eq!(seqs, vec![0, 1, 2]);
}

#[test]
fn create_marks_every_step_pending() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 3, 100).unwrap();
    let rows = step_rows(&journal, job);
    assert!(
        rows.iter()
            .all(|(_, state, detail)| state == "pending" && detail.is_none())
    );
}

#[test]
fn create_marks_job_running() {
    let mut journal = journal();
    let job = journal.create_job("move", b"plan", 1, 100).unwrap();
    assert_eq!(job_state(&journal, job), "running");
}

#[test]
fn create_keeps_plan_bytes_unchanged() {
    let mut journal = journal();
    let plan: Vec<u8> = (0..=255).chain([0, 0, 255]).collect();
    let job = journal.create_job("copy", &plan, 1, 100).unwrap();
    let stored: Vec<u8> = conn(&journal)
        .query_row("SELECT plan FROM job WHERE id = ?1", [job.0], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(stored, plan);
}

#[test]
fn create_gives_each_job_its_own_id() {
    let mut journal = journal();
    let first = journal.create_job("copy", b"a", 1, 100).unwrap();
    let second = journal.create_job("copy", b"b", 1, 100).unwrap();
    assert_ne!(first, second);
}

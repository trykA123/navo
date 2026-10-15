use super::*;

#[test]
fn crash_mid_job_leaves_committed_steps_for_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let (db, _) = Db::open(&path, false).unwrap();
    let mut journal = JobJournal::new(db).unwrap();
    let job = journal.create_job("copy", b"plan", 5, 100).unwrap();
    for seq in 0..3 {
        journal.mark_step(job, seq, StepState::Done, None).unwrap();
    }
    std::mem::forget(journal);

    let (db, report) = Db::open(&path, true).unwrap();
    assert!(report.checked);
    let journal = JobJournal::new(db).unwrap();
    let unfinished = journal.unfinished().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].id, job);
    let states: Vec<StepState> = unfinished[0].steps.iter().map(|s| s.state).collect();
    assert_eq!(
        states,
        vec![
            StepState::Done,
            StepState::Done,
            StepState::Done,
            StepState::Pending,
            StepState::Pending,
        ]
    );
}

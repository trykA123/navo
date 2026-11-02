use super::*;

#[test]
fn failed_holding_lists_failed_jobs_of_the_kind_with_undo_rows_newest_first_up_to_the_limit() {
    let mut journal = journal();
    let mut failed = Vec::new();
    for (plan, created_at) in [(b"old", 10), (b"new", 30), (b"mid", 20)] {
        let job = journal.create_job("save", plan, 1, created_at).unwrap();
        journal.record_held(job, b"proof", created_at).unwrap();
        journal.finish(job, JobState::Failed).unwrap();
        failed.push(job);
    }
    for (kind, state, held) in [
        ("save", JobState::Failed, false),
        ("save", JobState::Done, true),
        ("save", JobState::RestorePending, true),
        ("save", JobState::Running, true),
        ("copy", JobState::Failed, true),
    ] {
        let job = journal.create_job(kind, b"other", 1, 40).unwrap();
        if held {
            journal.record_held(job, b"proof", 40).unwrap();
        }
        journal.finish(job, state).unwrap();
    }
    assert_eq!(
        journal.failed_holding("save", 2).unwrap(),
        vec![(failed[1], b"new".to_vec()), (failed[2], b"mid".to_vec())]
    );
    assert_eq!(journal.failed_holding("save", 10).unwrap().len(), 3);
    assert!(journal.failed_holding("move", 10).unwrap().is_empty());
    assert_eq!(journal.count_failed_holding("save").unwrap(), 3);
    assert_eq!(journal.count_failed_holding("move").unwrap(), 0);
    journal.prune(0).unwrap();
    assert_eq!(journal.failed_holding("save", 10).unwrap().len(), 3);
}

use super::*;

#[test]
fn mark_stores_done_failed_and_skipped_with_detail() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 4, 100).unwrap();
    journal
        .mark_step(job, 0, StepState::Done, Some(b"ok"))
        .unwrap();
    journal
        .mark_step(job, 1, StepState::Failed, Some(b"disk full"))
        .unwrap();
    journal
        .mark_step(job, 2, StepState::Skipped, Some(b"exists"))
        .unwrap();
    assert_eq!(
        step_rows(&journal, job),
        vec![
            (0, "done".to_owned(), Some(b"ok".to_vec())),
            (1, "failed".to_owned(), Some(b"disk full".to_vec())),
            (2, "skipped".to_owned(), Some(b"exists".to_vec())),
            (3, "pending".to_owned(), None),
        ]
    );
}

#[test]
fn mark_twice_keeps_last_state_and_detail() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 1, 100).unwrap();
    journal
        .mark_step(job, 0, StepState::Failed, Some(b"locked"))
        .unwrap();
    journal.mark_step(job, 0, StepState::Done, None).unwrap();
    assert_eq!(step_rows(&journal, job), vec![(0, "done".to_owned(), None)]);
}

#[test]
fn mark_unknown_job_is_rejected_without_writing() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 1, 100).unwrap();
    let missing = JobId(job.0 + 1);
    let error = journal
        .mark_step(missing, 0, StepState::Done, Some(b"x"))
        .unwrap_err();
    assert!(
        matches!(error, JournalError::UnknownJob(id) if id == missing),
        "{error:?}"
    );
    assert_eq!(count(&journal, "SELECT count(*) FROM job_step"), 1);
    assert_eq!(
        step_rows(&journal, job),
        vec![(0, "pending".to_owned(), None)]
    );
}

#[test]
fn mark_unknown_step_is_rejected_without_writing() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 2, 100).unwrap();
    let error = journal
        .mark_step(job, 2, StepState::Done, Some(b"x"))
        .unwrap_err();
    assert!(
        matches!(error, JournalError::UnknownStep { job: j, seq: 2 } if j == job),
        "{error:?}"
    );
    assert_eq!(
        step_rows(&journal, job),
        vec![
            (0, "pending".to_owned(), None),
            (1, "pending".to_owned(), None)
        ]
    );
}

#[test]
fn a_batch_of_marks_lands_in_one_go() {
    let mut journal = journal();
    let job = journal.create_job("delete", b"plan", 3, 100).unwrap();
    journal
        .mark_steps(job, &[(2, StepState::Done), (0, StepState::Skipped)])
        .unwrap();
    assert_eq!(
        step_rows(&journal, job),
        vec![
            (0, "skipped".to_owned(), None),
            (1, "pending".to_owned(), None),
            (2, "done".to_owned(), None),
        ]
    );
}

#[test]
fn a_batch_with_an_unknown_step_writes_none_of_its_marks() {
    let mut journal = journal();
    let job = journal.create_job("delete", b"plan", 2, 100).unwrap();
    let error = journal
        .mark_steps(job, &[(0, StepState::Done), (5, StepState::Done)])
        .unwrap_err();
    assert!(
        matches!(error, JournalError::UnknownStep { seq: 5, .. }),
        "{error:?}"
    );
    assert_eq!(
        step_rows(&journal, job),
        vec![
            (0, "pending".to_owned(), None),
            (1, "pending".to_owned(), None),
        ]
    );
}

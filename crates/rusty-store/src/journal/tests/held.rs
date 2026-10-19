use super::*;

#[test]
fn held_records_come_back_in_the_order_they_were_written() {
    let mut journal = journal();
    let job = journal.create_job("copy", b"plan", 2, 100).unwrap();
    journal.record_held(job, b"first", 101).unwrap();
    journal.record_held(job, b"second", 102).unwrap();
    assert_eq!(
        journal.held(job).unwrap(),
        vec![b"first".to_vec(), b"second".to_vec()]
    );
}

#[test]
fn held_records_belong_to_their_job() {
    let mut journal = journal();
    let one = journal.create_job("copy", b"plan", 1, 100).unwrap();
    let two = journal.create_job("copy", b"plan", 1, 200).unwrap();
    journal.record_held(one, b"one", 101).unwrap();
    assert_eq!(journal.held(two).unwrap(), Vec::<Vec<u8>>::new());
}

#[test]
fn a_held_record_for_an_unknown_job_is_rejected() {
    let mut journal = journal();
    assert!(journal.record_held(JobId(42), b"x", 1).is_err());
}

#[test]
fn only_finished_jobs_with_held_records_are_listed() {
    let mut journal = journal();
    let running = journal.create_job("copy", b"plan", 1, 100).unwrap();
    let finished = journal.create_job("copy", b"plan", 1, 200).unwrap();
    let empty = journal.create_job("copy", b"plan", 1, 300).unwrap();
    journal.record_held(running, b"r", 101).unwrap();
    journal.record_held(finished, b"f", 201).unwrap();
    journal.record_held(finished, b"g", 202).unwrap();
    journal.finish(finished, JobState::Done).unwrap();
    journal.finish(empty, JobState::Done).unwrap();
    assert_eq!(journal.finished_holding().unwrap(), vec![finished]);
}

#[test]
fn forgetting_removes_only_that_jobs_records() {
    let mut journal = journal();
    let one = journal.create_job("copy", b"plan", 1, 100).unwrap();
    let two = journal.create_job("copy", b"plan", 1, 200).unwrap();
    journal.record_held(one, b"one", 101).unwrap();
    journal.record_held(two, b"two", 201).unwrap();
    assert_eq!(journal.forget_held(one).unwrap(), 1);
    assert_eq!(
        (journal.held(one).unwrap(), journal.held(two).unwrap()),
        (Vec::new(), vec![b"two".to_vec()])
    );
}

#[test]
fn prune_keeps_a_finished_job_that_still_holds_files() {
    let mut journal = journal();
    let holding = journal.create_job("copy", b"plan", 1, 100).unwrap();
    journal.record_held(holding, b"h", 101).unwrap();
    journal.finish(holding, JobState::Done).unwrap();
    let plain = journal.create_job("copy", b"plan", 1, 200).unwrap();
    journal.finish(plain, JobState::Done).unwrap();
    assert_eq!(journal.prune(0).unwrap(), 1);
    assert_eq!(journal.finished_holding().unwrap(), vec![holding]);
    journal.forget_held(holding).unwrap();
    assert_eq!(journal.prune(0).unwrap(), 1);
}

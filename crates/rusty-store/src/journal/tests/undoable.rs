use super::*;

fn finished(journal: &mut JobJournal, kind: &str, at: i64, state: JobState) -> JobId {
    let job = journal.create_job(kind, b"plan", 2, at).unwrap();
    journal.finish(job, state).unwrap();
    job
}

fn open_at(path: &std::path::Path) -> JobJournal {
    JobJournal::new(Db::open(path, false).unwrap().0).unwrap()
}

#[test]
fn newest_done_job_of_the_kind_wins() {
    let mut journal = journal();
    finished(&mut journal, "rename", 100, JobState::Done);
    let newest = finished(&mut journal, "rename", 300, JobState::Done);
    finished(&mut journal, "copy", 400, JobState::Done);
    journal
        .mark_step(newest, 1, StepState::Done, Some(b"d"))
        .unwrap();
    let job = journal.last_undoable("rename").unwrap().unwrap();
    assert_eq!(job.id, newest);
    assert_eq!(job.plan, b"plan".to_vec());
    assert_eq!(job.created_at, 300);
    assert_eq!(
        job.steps,
        vec![
            pending(0),
            JournalStep {
                seq: 1,
                state: StepState::Done,
                detail: Some(b"d".to_vec())
            }
        ]
    );
}

#[test]
fn nothing_to_undo_gives_none() {
    let mut journal = journal();
    assert!(journal.last_undoable("rename").unwrap().is_none());
    finished(&mut journal, "copy", 100, JobState::Done);
    assert!(journal.last_undoable("rename").unwrap().is_none());
}

#[test]
fn only_done_jobs_are_undoable() {
    let mut journal = journal();
    let done = finished(&mut journal, "rename", 100, JobState::Done);
    for (at, state) in [
        (200, JobState::Undone),
        (300, JobState::RolledBack),
        (400, JobState::Failed),
        (500, JobState::Cancelled),
        (700, JobState::Running),
    ] {
        finished(&mut journal, "rename", at, state);
    }
    assert_eq!(journal.last_undoable("rename").unwrap().unwrap().id, done);
}

#[test]
fn a_batch_whose_put_back_failed_stays_undoable() {
    let mut journal = journal();
    finished(&mut journal, "rename", 100, JobState::Done);
    let stuck = finished(&mut journal, "rename", 200, JobState::RestorePending);
    assert_eq!(journal.last_undoable("rename").unwrap().unwrap().id, stuck);
    journal.mark_job_undone(stuck).unwrap();
    assert_ne!(journal.last_undoable("rename").unwrap().unwrap().id, stuck);
}

#[test]
fn the_offer_says_whether_the_batch_is_restore_pending() {
    let mut journal = journal();
    let done = finished(&mut journal, "rename", 100, JobState::Done);
    let job = journal.last_undoable("rename").unwrap().unwrap();
    assert_eq!((job.id, job.restore_pending), (done, false));
    let stuck = finished(&mut journal, "rename", 200, JobState::RestorePending);
    let job = journal.last_undoable("rename").unwrap().unwrap();
    assert_eq!((job.id, job.restore_pending), (stuck, true));
}

#[test]
fn marking_undone_walks_back_through_older_batches() {
    let mut journal = journal();
    let first = finished(&mut journal, "rename", 100, JobState::Done);
    let second = finished(&mut journal, "rename", 200, JobState::Done);
    let third = finished(&mut journal, "rename", 300, JobState::Done);
    for expected in [third, second, first] {
        let job = journal.last_undoable("rename").unwrap().unwrap();
        assert_eq!(job.id, expected);
        journal.mark_job_undone(job.id).unwrap();
        assert_eq!(job_state(&journal, expected), "undone");
    }
    assert!(journal.last_undoable("rename").unwrap().is_none());
}

#[test]
fn marking_an_unknown_job_fails() {
    let mut journal = journal();
    let error = journal.mark_job_undone(JobId(99)).unwrap_err();
    assert!(matches!(error, JournalError::UnknownJob(JobId(99))));
}

#[test]
fn undo_state_survives_reopen_and_prune() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rusty.db");
    let (older, newer) = {
        let mut journal = open_at(&path);
        let older = finished(&mut journal, "rename", 100, JobState::Done);
        let newer = finished(&mut journal, "rename", 200, JobState::Done);
        journal.mark_job_undone(newer).unwrap();
        (older, newer)
    };
    let mut journal = open_at(&path);
    assert_eq!(journal.last_undoable("rename").unwrap().unwrap().id, older);
    journal.prune(0).unwrap();
    assert_eq!(journal.last_undoable("rename").unwrap().unwrap().id, older);
    assert_eq!(job_state(&journal, older), "done");
    assert_eq!(
        count(
            &journal,
            &format!("SELECT count(*) FROM job WHERE id = {}", newer.0)
        ),
        0
    );
}

#[test]
fn prune_keeps_twenty_rename_jobs_among_newer_other_jobs() {
    let mut journal = journal();
    let renames: Vec<JobId> = (0..25)
        .map(|at| finished(&mut journal, "rename", at, JobState::Done))
        .collect();
    for at in 1000..1050 {
        finished(&mut journal, "copy", at, JobState::Done);
    }
    journal.prune(50).unwrap();
    let kept = count(&journal, "SELECT count(*) FROM job WHERE kind = 'rename'");
    assert_eq!(kept, 20);
    assert_eq!(
        count(&journal, "SELECT count(*) FROM job WHERE kind = 'copy'"),
        50
    );
    assert_eq!(
        journal.last_undoable("rename").unwrap().unwrap().id,
        renames[24]
    );
    let oldest_kept: i64 = conn(&journal)
        .query_row(
            "SELECT min(created_at) FROM job WHERE kind = 'rename'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(oldest_kept, 5);
}

#[test]
fn prune_does_not_protect_undone_rename_jobs() {
    let mut journal = journal();
    for at in 0..5 {
        let job = finished(&mut journal, "rename", at, JobState::Done);
        journal.mark_job_undone(job).unwrap();
    }
    assert_eq!(journal.prune(0).unwrap(), 5);
}

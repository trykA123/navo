use super::*;

fn finished_jobs(journal: &mut JobJournal, created: &[i64]) -> Vec<JobId> {
    created
        .iter()
        .map(|&at| {
            let job = journal.create_job("copy", b"plan", 2, at).unwrap();
            journal.finish(job, JobState::Done).unwrap();
            job
        })
        .collect()
}

fn job_ids(journal: &JobJournal) -> Vec<i64> {
    let mut stmt = conn(journal)
        .prepare("SELECT id FROM job ORDER BY id")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn prune_keeps_newest_finished_jobs() {
    let mut journal = journal();
    let jobs = finished_jobs(&mut journal, &[100, 200, 300, 400]);
    assert_eq!(journal.prune(2).unwrap(), 2);
    assert_eq!(job_ids(&journal), vec![jobs[2].0, jobs[3].0]);
}

#[test]
fn prune_never_deletes_a_job_waiting_for_its_restore() {
    let mut journal = journal();
    let pending = journal.create_job("rename", b"plan", 1, 1).unwrap();
    journal.finish(pending, JobState::RestorePending).unwrap();
    finished_jobs(&mut journal, &[100, 200]);
    assert_eq!(journal.prune(0).unwrap(), 2);
    assert_eq!(job_ids(&journal), vec![pending.0]);
}

#[test]
fn prune_never_deletes_running_jobs() {
    let mut journal = journal();
    let running = journal.create_job("move", b"plan", 1, 1).unwrap();
    finished_jobs(&mut journal, &[100, 200]);
    assert_eq!(journal.prune(0).unwrap(), 2);
    assert_eq!(job_ids(&journal), vec![running.0]);
}

#[test]
fn prune_removes_step_rows_of_pruned_jobs() {
    let mut journal = journal();
    finished_jobs(&mut journal, &[100, 200, 300]);
    journal.prune(1).unwrap();
    assert_eq!(count(&journal, "SELECT count(*) FROM job_step"), 2);
    assert_eq!(
        count(
            &journal,
            "SELECT count(*) FROM job_step WHERE job_id NOT IN (SELECT id FROM job)"
        ),
        0
    );
}

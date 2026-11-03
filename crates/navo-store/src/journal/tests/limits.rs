use super::*;

const DAY: i64 = 24 * 60 * 60 * 1000;
const NOW: i64 = 100 * DAY;

fn finished(journal: &mut JobJournal, kind: &str, plan: &[u8], at: i64) -> JobId {
    let job = journal.create_job(kind, plan, 1, at).unwrap();
    journal.finish(job, JobState::Done).unwrap();
    job
}

fn exists(journal: &JobJournal, job: JobId) -> bool {
    count(
        journal,
        &format!("SELECT count(*) FROM job WHERE id = {}", job.0),
    ) == 1
}

fn standard() -> PruneLimits {
    PruneLimits::standard(NOW)
}

#[test]
fn age_and_byte_limits_never_drop_a_protected_job() {
    let mut journal = journal();
    let old = NOW - 90 * DAY;
    let big = vec![7_u8; 40 * 1024 * 1024];
    let running = journal.create_job("copy", &big, 1, old).unwrap();
    let pending = finished(&mut journal, "copy", &big, old + 1);
    journal.finish(pending, JobState::RestorePending).unwrap();
    let undo = finished(&mut journal, "copy", &big, old + 2);
    journal.record_held(undo, b"held", old + 2).unwrap();
    let retained = finished(&mut journal, "delete-retained", &big, old + 3);
    conn(&journal)
        .execute(
            "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, 'ready', x'00')",
            [retained.0],
        )
        .unwrap();
    let rename = finished(&mut journal, "rename", &big, old + 4);
    let moved = finished(&mut journal, "move", &big, old + 5);
    let newest = finished(&mut journal, "copy", &big, old + 6);
    journal.prune(standard()).unwrap();
    for job in [running, pending, undo, retained, rename, moved, newest] {
        assert!(exists(&journal, job), "job {job:?} is protected");
    }
}

#[test]
fn jobs_older_than_thirty_days_are_dropped() {
    let mut journal = journal();
    let old = finished(&mut journal, "copy", b"plan", NOW - 31 * DAY);
    let recent = finished(&mut journal, "copy", b"plan", NOW - 29 * DAY);
    let newest = finished(&mut journal, "copy", b"plan", NOW);
    assert_eq!(journal.prune(standard()).unwrap(), 1);
    assert!(!exists(&journal, old));
    assert!(exists(&journal, recent) && exists(&journal, newest));
}

#[test]
fn the_byte_limit_drops_the_oldest_jobs_first() {
    let mut journal = journal();
    let plan = vec![1_u8; 10 * 1024 * 1024];
    let jobs: Vec<JobId> = (0..5)
        .map(|at| finished(&mut journal, "copy", &plan, NOW - 5 + at))
        .collect();
    journal
        .mark_step(jobs[4], 0, StepState::Done, Some(&[2_u8; 1024]))
        .unwrap();
    assert_eq!(journal.prune(standard()).unwrap(), 2);
    assert_eq!(
        jobs.iter()
            .map(|job| exists(&journal, *job))
            .collect::<Vec<_>>(),
        [false, false, true, true, true]
    );
}

#[test]
fn step_details_count_toward_the_byte_limit() {
    let mut journal = journal();
    let older = finished(&mut journal, "copy", b"plan", NOW - 2);
    let newer = journal.create_job("copy", b"plan", 1, NOW - 1).unwrap();
    journal
        .mark_step(
            newer,
            0,
            StepState::Done,
            Some(&vec![3_u8; 33 * 1024 * 1024]),
        )
        .unwrap();
    journal.finish(newer, JobState::Done).unwrap();
    finished(&mut journal, "copy", b"plan", NOW);
    journal.prune(standard()).unwrap();
    assert!(!exists(&journal, older));
}

#[test]
fn the_count_limit_still_keeps_only_the_newest_fifty() {
    let mut journal = journal();
    let jobs: Vec<JobId> = (0..55)
        .map(|at| finished(&mut journal, "copy", b"plan", NOW - 100 + at))
        .collect();
    assert_eq!(journal.prune(standard()).unwrap(), 5);
    assert!(jobs[..5].iter().all(|job| !exists(&journal, *job)));
}

#[test]
fn undoable_moves_past_the_count_limit_are_dropped_as_before() {
    let mut journal = journal();
    let moved = finished(&mut journal, "move", b"plan", NOW - 200);
    for at in 0..50 {
        finished(&mut journal, "copy", b"plan", NOW - 100 + at);
    }
    journal.prune(standard()).unwrap();
    assert!(!exists(&journal, moved));
}

#[test]
fn pruning_everything_old_keeps_the_newest_id_so_ids_are_not_reused() {
    let mut journal = journal();
    finished(&mut journal, "copy", b"plan", NOW - 60 * DAY);
    let newest = finished(&mut journal, "copy", b"plan", NOW - 40 * DAY);
    journal.prune(standard()).unwrap();
    assert!(exists(&journal, newest));
    let next = journal.create_job("copy", b"plan", 1, NOW).unwrap();
    assert!(next.0 > newest.0);
}

use rusty_core::retention::RetentionId;

use super::*;

const DELETE: &str = "delete-retained-v1";
const COPY: &str = "copy-retained-v1";
const NOW: i64 = 1_000;

fn kinds() -> UndoKinds<'static> {
    UndoKinds {
        rename: "rename",
        moves: "move",
        retained: &[DELETE, COPY],
        now_ms: NOW,
    }
}

fn finished(journal: &mut JobJournal, kind: &str, at: i64, state: JobState) -> JobId {
    let job = journal.create_job(kind, b"plan", 1, at).unwrap();
    journal.finish(job, state).unwrap();
    job
}

fn with_done_step(journal: &mut JobJournal, kind: &str, at: i64, state: JobState) -> JobId {
    let job = finished(journal, kind, at, state);
    journal.mark_step(job, 0, StepState::Done, None).unwrap();
    job
}

fn entry(journal: &JobJournal, job: JobId, state: &str, expires_at: i64) -> RetentionId {
    let record = serde_json::json!({ "source": "/home/a.txt", "expires_at": expires_at });
    conn(journal)
        .execute(
            "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, ?2, ?3)",
            params![job.0, state, serde_json::to_vec(&record).unwrap()],
        )
        .unwrap();
    RetentionId(conn(journal).last_insert_rowid())
}

fn newest(journal: &JobJournal) -> Option<Undoable> {
    journal.newest_undoable(kinds()).unwrap()
}

fn job_of(undoable: Option<Undoable>) -> Option<JobId> {
    undoable.map(|found| match found {
        Undoable::Rename(job) => job.id,
        Undoable::Retained { id, .. } => id,
    })
}

#[test]
fn the_newest_job_of_either_kind_is_offered_first() {
    let mut journal = journal();
    let rename = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let delete = finished(&mut journal, DELETE, 200, JobState::Done);
    let (first, second) = (
        entry(&journal, delete, "ready", NOW + 1),
        entry(&journal, delete, "ready", NOW + 1),
    );
    assert_eq!(
        newest(&journal),
        Some(Undoable::Retained {
            id: delete,
            created_at: 200,
            entries: vec![first, second],
        })
    );
    journal.mark_job_undone(delete).unwrap();
    assert!(matches!(newest(&journal), Some(Undoable::Rename(job)) if job.id == rename));
    let later = with_done_step(&mut journal, "rename", 300, JobState::Done);
    assert_eq!(job_of(newest(&journal)), Some(later));
}

#[test]
fn a_retained_job_with_nothing_left_to_restore_is_skipped() {
    let mut journal = journal();
    let rename = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let delete = finished(&mut journal, DELETE, 200, JobState::Done);
    for state in ["restored", "purged", "aborted"] {
        entry(&journal, delete, state, NOW + 1);
    }
    entry(&journal, delete, "ready", NOW);
    let overwrite = finished(&mut journal, COPY, 300, JobState::Done);
    assert_eq!(job_of(newest(&journal)), Some(rename));
    let live = entry(&journal, overwrite, "ready", NOW + 1);
    assert_eq!(
        newest(&journal),
        Some(Undoable::Retained {
            id: overwrite,
            created_at: 300,
            entries: vec![live],
        })
    );
}

#[test]
fn only_live_unexpired_entries_are_offered() {
    let mut journal = journal();
    let delete = finished(&mut journal, DELETE, 100, JobState::Done);
    entry(&journal, delete, "purged", NOW + 1);
    let kept = entry(&journal, delete, "ready", NOW + 1);
    entry(&journal, delete, "ready", NOW - 1);
    let odd = entry(&journal, delete, "conflict", NOW + 1);
    assert_eq!(
        newest(&journal),
        Some(Undoable::Retained {
            id: delete,
            created_at: 100,
            entries: vec![kept, odd],
        })
    );
}

#[test]
fn a_corrupt_record_is_not_offered_and_does_not_break_the_query() {
    let mut journal = journal();
    let rename = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let delete = finished(&mut journal, DELETE, 200, JobState::Done);
    conn(&journal)
        .execute(
            "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, 'ready', X'ff')",
            [delete.0],
        )
        .unwrap();
    assert_eq!(job_of(newest(&journal)), Some(rename));
}

#[test]
fn unfinished_failed_and_other_kinds_are_not_offered() {
    let mut journal = journal();
    for (kind, state) in [
        (DELETE, JobState::Failed),
        (DELETE, JobState::Running),
        (DELETE, JobState::RestorePending),
        (DELETE, JobState::Undone),
        ("delete", JobState::Done),
        ("copy", JobState::Done),
    ] {
        let job = finished(&mut journal, kind, 100, state);
        entry(&journal, job, "ready", NOW + 1);
    }
    assert_eq!(newest(&journal), None);
}

#[test]
fn a_rename_keeps_its_plan_steps_and_restore_pending_flag() {
    let mut journal = journal();
    let stuck = with_done_step(&mut journal, "rename", 100, JobState::RestorePending);
    let Some(Undoable::Rename(job)) = newest(&journal) else {
        panic!("a rename offer");
    };
    assert_eq!((job.id, job.restore_pending), (stuck, true));
    assert_eq!(job.plan, b"plan".to_vec());
    assert_eq!(
        job.steps,
        vec![JournalStep {
            seq: 0,
            state: StepState::Done,
            detail: None,
        }]
    );
}

#[test]
fn the_retained_offer_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rusty.db");
    let (delete, live) = {
        let mut journal = JobJournal::new(Db::open(&path, false).unwrap().0).unwrap();
        let delete = finished(&mut journal, DELETE, 100, JobState::Done);
        (delete, entry(&journal, delete, "ready", NOW + 1))
    };
    let journal = JobJournal::new(Db::open(&path, false).unwrap().0).unwrap();
    assert_eq!(
        newest(&journal),
        Some(Undoable::Retained {
            id: delete,
            created_at: 100,
            entries: vec![live],
        })
    );
}

#[test]
fn a_failed_job_with_a_live_done_step_is_offered() {
    let mut journal = journal();
    let failed = finished(&mut journal, "rename", 100, JobState::Failed);
    journal
        .mark_step(failed, 0, StepState::Done, Some(b"identity"))
        .unwrap();
    assert_eq!(job_of(newest(&journal)), Some(failed));
}

#[test]
fn a_fully_undone_failed_job_does_not_hide_an_older_live_job() {
    let mut journal = journal();
    let older = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let failed = with_done_step(&mut journal, "rename", 200, JobState::Failed);
    journal.mark_undone(failed, 0).unwrap();
    assert_eq!(job_of(newest(&journal)), Some(older));
}

#[test]
fn a_fully_undone_done_job_does_not_hide_an_older_live_job() {
    let mut journal = journal();
    let older = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let newer = with_done_step(&mut journal, "rename", 200, JobState::Done);
    journal.mark_undone(newer, 0).unwrap();
    assert_eq!(job_of(newest(&journal)), Some(older));
}

#[test]
fn a_fully_undone_restore_pending_job_does_not_hide_an_older_live_job() {
    let mut journal = journal();
    let older = with_done_step(&mut journal, "rename", 100, JobState::Done);
    let newer = with_done_step(&mut journal, "rename", 200, JobState::RestorePending);
    journal.mark_undone(newer, 0).unwrap();
    assert_eq!(job_of(newest(&journal)), Some(older));
}

fn moved(journal: &mut JobJournal, at: i64, state: JobState, details: &[Option<&[u8]>]) -> JobId {
    let steps = u32::try_from(details.len()).unwrap();
    let job = journal.create_job("move", b"plan", steps, at).unwrap();
    journal.finish(job, state).unwrap();
    for (seq, detail) in (0_u32..).zip(details) {
        journal
            .mark_step(job, seq, StepState::Done, *detail)
            .unwrap();
    }
    job
}

#[test]
fn a_move_with_an_identity_on_every_done_step_is_offered_with_its_plan() {
    let mut journal = journal();
    with_done_step(&mut journal, "rename", 100, JobState::Done);
    let mut offered = Vec::new();
    for (at, state) in [
        (200, JobState::Done),
        (300, JobState::Failed),
        (400, JobState::Cancelled),
        (500, JobState::RestorePending),
    ] {
        let job = moved(
            &mut journal,
            at,
            state,
            &[Some(b"identity"), Some(b"identity")],
        );
        let Some(Undoable::Rename(found)) = newest(&journal) else {
            panic!("a move offer");
        };
        offered.push((found.id == job, found.plan == b"plan".to_vec()));
    }
    assert_eq!(offered, vec![(true, true); 4]);
}

#[test]
fn a_move_with_a_done_step_lacking_an_identity_is_never_offered() {
    let mut journal = journal();
    let rename = with_done_step(&mut journal, "rename", 100, JobState::Done);
    moved(
        &mut journal,
        200,
        JobState::Done,
        &[Some(b"identity"), None],
    );
    moved(&mut journal, 300, JobState::Done, &[None]);
    let undone = moved(&mut journal, 400, JobState::Done, &[Some(b"identity")]);
    journal.mark_undone(undone, 0).unwrap();
    moved(&mut journal, 500, JobState::Running, &[Some(b"identity")]);
    moved(&mut journal, 600, JobState::Undone, &[Some(b"identity")]);
    assert_eq!(job_of(newest(&journal)), Some(rename));
}

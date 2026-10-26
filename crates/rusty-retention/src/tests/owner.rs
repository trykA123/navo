use super::*;
use rusty_store::{JobJournal, JobState};

fn owner(profile: &Path) -> JobId {
    let (db, _) = Db::open(&profile.join("ferrite.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .create_job("delete-retained-v1", b"plan", 1, 100)
        .unwrap()
}

fn owned(vault: &mut Vault<'_>, job: JobId, source: &Location) -> RetentionEntry {
    run(|run| {
        vault.capture(
            CaptureRequest {
                source,
                policy: RetentionPolicy::new(3, 1000, 0).unwrap(),
                created_at: 100,
                job: Some(job),
            },
            run,
        )
    })
    .unwrap()
}

#[test]
fn only_the_active_owner_can_restore_a_pinned_capture() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let wrong = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    assert!(restore(&mut vault, entry.id, &entry.record.source).is_err());
    assert!(run(|run| vault.rollback(entry.id, wrong, run)).is_err());
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
    assert_eq!(
        run(|run| vault.rollback(entry.id, job, run)).unwrap().state,
        RetainedState::Restored
    );
    assert!(fs.metadata(&entry.record.source).is_ok());
}

#[test]
fn occupied_source_is_preserved_and_owner_rollback_can_retry_after_restart() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    fs.add_file("home/a", 99);
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 99);
    drop(vault);
    fs.remove_file(&entry.record.source).unwrap();
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        run(|run| vault.rollback(entry.id, job, run)).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 3);
}

#[test]
fn terminal_owner_cannot_use_recovery_authority() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Done)
        .unwrap();
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
}

#[test]
fn repinning_the_owner_during_purge_verification_prevents_the_next_delete() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/tree"));
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    let mut writer = JobJournal::new(db).unwrap();
    writer.finish(job, JobState::Done).unwrap();
    let (observer, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    let mut repinned = false;
    let mut report = |_: RetentionProgress| {
        let state: String = observer
            .connection()
            .query_row(
                "SELECT state FROM retained_entry WHERE id = ?1",
                [entry.id.0],
                |row| row.get(0),
            )
            .unwrap();
        if state == "purging" && !repinned {
            writer.finish(job, JobState::RestorePending).unwrap();
            repinned = true;
        }
    };
    let cancel = CancelToken::new();
    assert!(
        vault
            .purge(entry.id, &mut RetentionRun::new(&cancel, &mut report))
            .is_err()
    );
    assert!(repinned);
    let held = entry
        .record
        .held()
        .unwrap()
        .join(&ProviderPath::from_text("a"))
        .unwrap();
    assert_eq!(fs.metadata(&held).unwrap().size, 3);
    assert!(vault.get(entry.id).unwrap().pinned);
}

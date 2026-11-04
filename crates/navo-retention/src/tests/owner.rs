use super::*;
use navo_store::{JobJournal, JobState};

fn owner(profile: &Path) -> JobId {
    let (db, _) = Db::open(&profile.join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .create_job("delete-retained-v1", b"plan", 1, 100)
        .unwrap()
}

fn owned(vault: &mut Vault<'_>, job: JobId, source: &Location) -> RetentionEntry {
    run(|run| {
        vault.capture(
            &CaptureRequest {
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
    let conflicted = vault.get(entry.id).unwrap();
    assert_eq!(conflicted.state, RetainedState::Conflict);
    assert_eq!(conflicted.target.unwrap().location, entry.record.source);
    assert_eq!(fs.metadata(&entry.record.held().unwrap()).unwrap().size, 3);
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
fn a_rollback_that_can_never_finish_fails_with_a_permanent_error() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    fs.add_file("home/docs/b", 4);
    fs.add_file("home/c", 5);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let occupied = owned(&mut vault, job, &fs.location("home/a"));
    let orphaned = owned(&mut vault, job, &fs.location("home/docs/b"));
    let vanished = owned(&mut vault, job, &fs.location("home/c"));
    fs.add_file("home/a", 99);
    fs.remove("home/docs");
    fs.remove(&vanished.record.held().unwrap().path().to_text());
    for entry in [&occupied, &orphaned, &vanished] {
        for attempt in 0..2 {
            let error = run(|run| vault.rollback(entry.id, job, run)).unwrap_err();
            assert!(
                vault.needs_attention(&error, entry),
                "{} attempt {attempt}: {error}",
                entry.record.source.as_str()
            );
        }
    }
    assert_eq!(fs.metadata(&occupied.record.source).unwrap().size, 99);
    assert_eq!(
        fs.metadata(&occupied.record.held().unwrap()).unwrap().size,
        3
    );
    assert_eq!(
        fs.metadata(&orphaned.record.held().unwrap()).unwrap().size,
        4
    );
}

#[test]
fn terminal_owner_cannot_use_recovery_authority() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    let (db, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
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
    let (db, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
    let mut writer = JobJournal::new(db).unwrap();
    writer.finish(job, JobState::Done).unwrap();
    let (observer, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
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

#[test]
fn owner_rollback_of_a_changed_payload_needs_attention_and_retries_after_restart() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    let payload = entry.record.held().unwrap().path().to_text();
    fs.fault(
        &payload,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Restoring);
    fs.rename_entry(&payload, "home/moved");
    fs.add_file(&payload, 9);
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    let conflicted = vault.get(entry.id).unwrap();
    assert_eq!(conflicted.state, RetainedState::Conflict);
    assert!(
        conflicted.pinned,
        "the job keeps its entry while it is pending"
    );
    assert!(fs.metadata(&entry.record.source).is_err());
    drop(vault);
    fs.remove(&payload);
    fs.rename_entry("home/moved", &payload);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        run(|run| vault.rollback(entry.id, job, run)).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 3);
}

#[test]
fn a_conflict_left_by_a_rollback_outlives_the_age_and_size_limits() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(1000));
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    fs.add_file("home/a", 99);
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    drop(vault);
    let (db, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Failed)
        .unwrap();
    let mut vault = open(profile.path(), &fs);
    let conflicted = vault.get(entry.id).unwrap();
    assert_eq!(conflicted.state, RetainedState::Conflict);
    assert!(conflicted.pinned && conflicted.kept);
    let at = fs.location("home");
    let request = MaintenanceRequest {
        at: &at,
        now: 30 * DAY_MS,
        policy: RetentionPolicy::new(1, 1, 0).unwrap(),
        max_purges: 256,
    };
    for _ in 0..2 {
        let report = run(|run| vault.maintain_volume(&request, run)).unwrap();
        assert!(report.purged.is_empty(), "{report:?}");
    }
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    assert_eq!(fs.metadata(&entry.record.held().unwrap()).unwrap().size, 3);
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 99);
    let deleted = run(|run| vault.delete(entry.id, run)).unwrap();
    assert_eq!(deleted.state, RetainedState::Purged);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_err());
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 99);
}

/// Fails `job` after its rollback of `home/docs/b` found the original folder gone.
fn kept_ready(profile: &Path, fs: &FakeFs) -> RetentionEntry {
    fs.set_available_space("home", Some(1000));
    fs.add_file("home/docs/b", 4);
    let job = owner(profile);
    let mut vault = open(profile, fs);
    let entry = owned(&mut vault, job, &fs.location("home/docs/b"));
    fs.remove("home/docs");
    let error = run(|run| vault.rollback(entry.id, job, run)).unwrap_err();
    assert!(vault.needs_attention(&error, &entry), "{error}");
    drop(vault);
    let (db, _) = Db::open(&profile.join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Failed)
        .unwrap();
    entry
}

#[test]
fn a_ready_entry_of_a_failed_delete_outlives_the_age_and_size_limits() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let entry = kept_ready(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    let kept = vault.get(entry.id).unwrap();
    assert_eq!(kept.state, RetainedState::Ready);
    assert!(kept.pinned && kept.kept);
    let at = fs.location("home");
    let request = MaintenanceRequest {
        at: &at,
        now: 30 * DAY_MS,
        policy: RetentionPolicy::new(1, 1, 0).unwrap(),
        max_purges: 256,
    };
    for _ in 0..2 {
        let report = run(|run| vault.maintain_volume(&request, run)).unwrap();
        assert!(report.purged.is_empty(), "{report:?}");
    }
    assert!(matches!(purge(&mut vault, entry.id), Err(Error::Pinned(_))));
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
    assert_eq!(fs.metadata(&entry.record.held().unwrap()).unwrap().size, 4);
    let deleted = run(|run| vault.delete(entry.id, run)).unwrap();
    assert_eq!(deleted.state, RetainedState::Purged);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_err());
}

fn kept_pair_after_interrupted_delete(profile: &Path, fs: &FakeFs) -> (JobId, [RetentionEntry; 2]) {
    fs.set_available_space("home", Some(1000));
    fs.add_file("home/ready", 7);
    fs.add_file("home/tree/a", 3);
    fs.add_file("home/tree/b", 5);
    let job = owner(profile);
    let mut vault = open(profile, fs);
    let ready = owned(&mut vault, job, &fs.location("home/ready"));
    let purging = owned(&mut vault, job, &fs.location("home/tree"));
    let (db, _) = Db::open(&profile.join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Failed)
        .unwrap();
    let held = purging.record.held().unwrap();
    let remaining = held.join(&ProviderPath::from_text("b")).unwrap();
    let removed = held.join(&ProviderPath::from_text("a")).unwrap();
    fs.fault(
        &remaining.path().to_text(),
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    assert!(matches!(
        run(|run| vault.delete(purging.id, run)),
        Err(Error::Fs(error)) if error.kind == FsErrorKind::Locked
    ));
    let purging = vault.get(purging.id).unwrap();
    assert_eq!(purging.state, RetainedState::Purging);
    assert!(purging.pinned && purging.kept);
    assert_eq!(fs.metadata(&remaining).unwrap().size, 5);
    assert!(fs.metadata(&removed).is_err());
    (job, [ready, purging])
}

#[test]
fn maintenance_completes_a_kept_purge_and_preserves_its_ready_sibling() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (_, [ready, purging]) = kept_pair_after_interrupted_delete(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    let at = fs.location("home");
    let request = MaintenanceRequest {
        at: &at,
        now: 30 * DAY_MS,
        policy: RetentionPolicy::new(1, 1, 0).unwrap(),
        max_purges: 256,
    };
    let report = run(|run| vault.maintain_volume(&request, run)).unwrap();
    assert_eq!(report.purged, vec![purging.id]);
    assert!(report.skipped.is_empty());
    assert_eq!(report.remaining_bytes, 7);
    assert!(report.quota_unmet);
    assert_eq!(vault.get(purging.id).unwrap().state, RetainedState::Purged);
    assert!(fs.metadata(&purging.record.container).is_err());
    let ready = vault.get(ready.id).unwrap();
    assert_eq!(ready.state, RetainedState::Ready);
    assert!(ready.pinned && ready.kept);
    assert_eq!(fs.metadata(&ready.record.held().unwrap()).unwrap().size, 7);
}

#[test]
fn maintenance_never_admits_a_kept_purge_after_its_owner_is_running_again() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (job, [_, purging]) = kept_pair_after_interrupted_delete(profile.path(), &fs);
    let (db, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Running)
        .unwrap();
    let mut vault = open(profile.path(), &fs);
    let at = fs.location("home");
    let request = MaintenanceRequest {
        at: &at,
        now: 30 * DAY_MS,
        policy: RetentionPolicy::new(1, 1, 0).unwrap(),
        max_purges: 256,
    };
    let remaining = purging
        .record
        .held()
        .unwrap()
        .join(&ProviderPath::from_text("b"))
        .unwrap();
    let removes = fs.calls(&remaining.path().to_text(), FakeOp::Remove);
    let report = run(|run| vault.maintain_volume(&request, run)).unwrap();
    assert!(report.purged.is_empty() && report.skipped.is_empty());
    assert!(!report.budget_exhausted);
    let entry = vault.get(purging.id).unwrap();
    assert_eq!(entry.state, RetainedState::Purging);
    assert!(entry.pinned && !entry.kept);
    assert_eq!(fs.metadata(&remaining).unwrap().size, 5);
    assert_eq!(
        fs.calls(&remaining.path().to_text(), FakeOp::Remove),
        removes
    );
    let summary = vault.catalog.summaries_after(0, 10).unwrap();
    assert!(summary.iter().all(|entry| entry.attempts() == 0));
}

#[test]
fn the_user_can_restore_a_ready_entry_of_a_failed_delete_elsewhere() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let entry = kept_ready(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    let target = fs.location("home/b (restored)");
    let restored = restore(&mut vault, entry.id, &target).unwrap();
    assert_eq!(restored.state, RetainedState::Restored);
    assert_eq!(fs.metadata(&target).unwrap().size, 4);
}

#[test]
fn deleting_a_conflict_whose_held_copy_changed_keeps_every_file() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 3);
    let job = owner(profile.path());
    let mut vault = open(profile.path(), &fs);
    let entry = owned(&mut vault, job, &fs.location("home/a"));
    fs.add_file("home/a", 99);
    assert!(run(|run| vault.rollback(entry.id, job, run)).is_err());
    let held = entry.record.held().unwrap();
    fs.add_file(&held.path().to_text(), 7);
    drop(vault);
    let (db, _) = Db::open(&profile.path().join("navo.db"), false).unwrap();
    JobJournal::new(db)
        .unwrap()
        .finish(job, JobState::Failed)
        .unwrap();
    let mut vault = open(profile.path(), &fs);
    assert!(run(|run| vault.delete(entry.id, run)).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    assert_eq!(fs.metadata(&held).unwrap().size, 7);
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 99);
}

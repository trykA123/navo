use super::*;

#[test]
fn a_clean_restart_keeps_retained_files_available_for_restore() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(vault.get(entry.id).unwrap(), entry);
    restore(&mut vault, entry.id, &fs.location("home/a")).unwrap();
}

#[test]
fn an_interrupted_preparation_does_not_start_a_new_source_move() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.fault(
        "home/a",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = open(profile.path(), &fs);
    let source = fs.location("home/a");
    assert!(
        run(|run| vault.capture(
            CaptureRequest {
                source: &source,
                policy: RetentionPolicy::new(1, 100, 0).unwrap(),
                created_at: 0,
                job: None
            },
            run
        ))
        .is_err()
    );
    let entry = vault.list_after(0, 10).unwrap().remove(0);
    assert_eq!(entry.state, RetainedState::Preparing);
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Aborted
    );
    assert_eq!(fs.calls("home/a", FakeOp::Rename), 1);
    assert!(fs.metadata(&source).is_ok());
}

#[test]
fn a_completed_capture_is_recognized_after_its_final_database_commit_was_lost() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    raw_state(profile.path(), entry.id, "preparing");
    fs.add_file("home/a", 33);
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Ready
    );
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 33);
}

#[test]
fn a_completed_restore_is_recognized_without_overwriting_the_target() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let source = fs.location("home/a");
    let entry = capture(&mut vault, &source);
    begin_restore(&mut vault, entry.id, &source);
    fs.rename(&entry.record.held().unwrap(), &source).unwrap();
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.metadata(&source).unwrap().size, 7);
}

#[test]
fn an_interrupted_restore_keeps_a_replacement_target_and_marks_the_conflict() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let source = fs.location("home/a");
    let entry = capture(&mut vault, &source);
    begin_restore(&mut vault, entry.id, &source);
    fs.add_file("home/a", 33);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Conflict
    );
    assert_eq!(fs.metadata(&source).unwrap().size, 33);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
}

#[test]
fn one_profile_allows_only_one_retention_engine_at_a_time() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let first = open(profile.path(), &fs);
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    assert!(Vault::new(db, profile.path(), &fs).is_err());
    drop(first);
    open(profile.path(), &fs);
}

fn unrecorded_container(profile: &Path, fs: &FakeFs) -> RetentionEntry {
    fs.add_file("home/a", 7);
    fs.fault(
        "home/a",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = open(profile, fs);
    assert!(
        run(|run| vault.capture(
            CaptureRequest {
                source: &fs.location("home/a"),
                policy: Default::default(),
                created_at: 0,
                job: None,
            },
            run
        ))
        .is_err()
    );
    let mut entry = vault.list_after(0, 10).unwrap().remove(0);
    drop(vault);
    entry.record.container_identity = None;
    let (db, _) = Db::open(&profile.join("ferrite.db"), false).unwrap();
    db.connection().execute("UPDATE retained_entry SET record = CAST(json_set(CAST(record AS TEXT), '$.container_identity', NULL) AS BLOB) WHERE id = ?1",
        [entry.id.0]).unwrap();
    entry
}

#[test]
fn crash_before_container_identity_commit_aborts_an_empty_preparation() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let entry = unrecorded_container(profile.path(), &fs);
    let original = fs.file_identity(&entry.record.source).unwrap();
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Aborted
    );
    assert!(fs.metadata(&entry.record.container).is_err());
    assert_eq!(fs.file_identity(&entry.record.source).unwrap(), original);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Aborted
    );
    assert_eq!(fs.calls("home/a", FakeOp::Rename), 1);
}

#[test]
fn an_unrecorded_nonempty_container_keeps_its_unexpected_bytes() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let entry = unrecorded_container(profile.path(), &fs);
    let foreign = format!("{}/foreign", entry.record.container.path().to_text());
    fs.add_file(&foreign, 99);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Conflict
    );
    assert_eq!(fs.metadata(&fs.location(&foreign)).unwrap().size, 99);
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 7);
}

#[test]
fn an_unrecorded_container_rechecks_the_original_after_progress_callbacks() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let entry = unrecorded_container(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    let mut progress = |_: RetentionProgress| {
        fs.remove("home/a");
        fs.add_file("home/a", 99);
    };
    let result = vault
        .reconcile(
            entry.id,
            &mut RetentionRun::new(&CancelToken::new(), &mut progress),
        )
        .unwrap();
    assert_eq!(result.state, RetainedState::Conflict);
    assert_eq!(fs.metadata(&entry.record.source).unwrap().size, 99);
    assert!(fs.metadata(&entry.record.container).is_ok());
}

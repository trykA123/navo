use super::*;

#[test]
fn a_capture_renames_the_source_without_copying_and_can_be_restored() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let original = fs.location("home/a");
    let identity = fs.file_identity(&original).unwrap();
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &original);
    assert_eq!(entry.state, RetainedState::Ready);
    assert_eq!(entry.record.expires_at, 100 + 3 * DAY_MS);
    assert!(fs.metadata(&original).is_err());
    assert_eq!(
        fs.file_identity(&entry.record.held().unwrap()).unwrap(),
        identity
    );
    assert_eq!(fs.calls("home/a", FakeOp::Copy), 0);
    assert_eq!(
        restore(&mut vault, entry.id, &original).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.file_identity(&original).unwrap(), identity);
}

#[test]
fn restore_keeps_a_new_original_file_and_allows_a_different_target() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    fs.add_file("home/a", 33);
    assert!(restore(&mut vault, entry.id, &fs.location("home/a")).is_err());
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 33);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
    restore(&mut vault, entry.id, &fs.location("home/restored")).unwrap();
    assert_eq!(fs.metadata(&fs.location("home/restored")).unwrap().size, 7);
}

#[test]
fn a_tree_is_preserved_with_one_rename_and_restores_its_children() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    fs.add_file("home/tree/nested/b", 11);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    assert_eq!(entry.record.bytes().unwrap(), 18);
    assert_eq!(entry.record.nodes.len(), 4);
    assert_eq!(fs.calls("home/tree", FakeOp::Rename), 1);
    restore(&mut vault, entry.id, &fs.location("home/tree")).unwrap();
    assert_eq!(
        fs.metadata(&fs.location("home/tree/nested/b"))
            .unwrap()
            .size,
        11
    );
}

#[test]
fn unknown_volume_identity_and_nested_mounts_leave_the_source_in_place() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/mount/a", 7);
    fs.set_volume("home/tree/mount", 999);
    let mut vault = open(profile.path(), &fs);
    let source = fs.location("home/tree");
    let result = run(|run| {
        vault.capture(
            CaptureRequest {
                source: &source,
                policy: RetentionPolicy::new(1, 100, 0).unwrap(),
                created_at: 0,
                job: None,
            },
            run,
        )
    });
    assert!(result.is_err());
    assert_eq!(fs.calls("home/tree", FakeOp::Rename), 0);
    assert_eq!(fs.calls("home/tree/mount", FakeOp::List), 0);
    assert!(vault.list_after(0, 10).unwrap().is_empty());
    fs.set_volume_serial("home", None);
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
}

#[test]
fn cancellation_during_the_snapshot_never_creates_a_capture_intent() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    let source = fs.location("home/tree");
    let mut vault = open(profile.path(), &fs);
    let cancel = CancelToken::new();
    let mut progress = |_| {
        cancel.cancel();
    };
    let mut run = RetentionRun::new(&cancel, &mut progress);
    assert!(
        vault
            .capture(
                CaptureRequest {
                    source: &source,
                    policy: RetentionPolicy::new(1, 100, 0).unwrap(),
                    created_at: 0,
                    job: None
                },
                &mut run
            )
            .is_err()
    );
    assert!(vault.list_after(0, 10).unwrap().is_empty());
    assert!(fs.metadata(&source).is_ok());
}

#[test]
fn an_active_owner_pins_its_retained_files_until_the_job_finishes() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    db.connection()
        .execute(
            "INSERT INTO job VALUES (1, 'delete', 'running', 0, X'00')",
            [],
        )
        .unwrap();
    let mut vault = Vault::new(db, profile.path(), &fs).unwrap();
    let source = fs.location("home/a");
    let entry = run(|run| {
        vault.capture(
            CaptureRequest {
                source: &source,
                policy: RetentionPolicy::new(1, 100, 0).unwrap(),
                created_at: 0,
                job: Some(JobId(1)),
            },
            run,
        )
    })
    .unwrap();
    assert!(matches!(purge(&mut vault, entry.id), Err(Error::Pinned(_))));
    assert!(matches!(
        restore(&mut vault, entry.id, &source),
        Err(Error::Pinned(_))
    ));
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    db.connection()
        .execute("UPDATE job SET state = 'done' WHERE id = 1", [])
        .unwrap();
    restore(&mut vault, entry.id, &source).unwrap();
    assert!(fs.metadata(&source).is_ok());
}

#[test]
fn a_same_volume_mount_is_rejected_before_listing_its_contents() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/bind/a", 7);
    fs.set_mount_identity("home/tree/bind", Some(999));
    let source = fs.location("home/tree");
    let mut vault = open(profile.path(), &fs);
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
    assert_eq!(fs.calls("home/tree/bind", FakeOp::List), 0);
    assert!(vault.list_after(0, 10).unwrap().is_empty());
    assert!(fs.metadata(&fs.location("home/tree/bind/a")).is_ok());
}

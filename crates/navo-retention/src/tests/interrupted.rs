use super::*;

fn held(entry: &RetentionEntry) -> Location {
    entry.record.held().unwrap()
}

fn fail_next_rename(fs: &FakeFs, entry: &RetentionEntry, fault: Fault) {
    let path = held(entry).path().to_text();
    let next = fs.calls(&path, FakeOp::Rename) + 1;
    fs.fault(&path, FakeOp::Rename, next, fault);
}

fn interrupted(fault: Fault) -> (tempfile::TempDir, FakeFs, RetentionEntry) {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    fail_next_rename(&fs, &entry, fault);
    assert!(restore(&mut vault, entry.id, &fs.location("home/a")).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Restoring);
    drop(vault);
    (profile, fs, entry)
}

#[test]
fn a_restore_that_failed_before_its_rename_restores_when_retried_after_a_restart() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let mut vault = open(profile.path(), &fs);
    let target = fs.location("home/a");
    let restored = restore(&mut vault, entry.id, &target).unwrap();
    assert_eq!(restored.state, RetainedState::Restored);
    assert_eq!(fs.metadata(&target).unwrap().size, 7);
    assert!(fs.metadata(&held(&entry)).is_err());
}

#[test]
fn maintenance_returns_an_interrupted_restore_to_ready_without_moving_anything() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let mut vault = open(profile.path(), &fs);
    let at = fs.location("home");
    let report = run(|run| {
        vault.maintain_volume(
            &MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 100,
                max_purges: 8,
            },
            run,
        )
    })
    .unwrap();
    assert!(report.purged.is_empty() && report.skipped.is_empty());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
    assert_eq!(fs.metadata(&held(&entry)).unwrap().size, 7);
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert_eq!(
        restore(&mut vault, entry.id, &fs.location("home/a"))
            .unwrap()
            .state,
        RetainedState::Restored
    );
}

#[test]
fn a_retried_restore_finishes_one_whose_rename_landed_but_was_not_recorded() {
    let (profile, fs, entry) = interrupted(Fault::RenameThenFail(FsErrorKind::Locked));
    let mut vault = open(profile.path(), &fs);
    let elsewhere = fs.location("home/b");
    let restored = restore(&mut vault, entry.id, &elsewhere).unwrap();
    assert_eq!(restored.state, RetainedState::Restored);
    assert_eq!(restored.target.unwrap().location, fs.location("home/a"));
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 7);
    assert!(fs.metadata(&elsewhere).is_err());
}

#[test]
fn a_retried_purge_never_removes_a_restore_that_landed_but_was_not_recorded() {
    let (profile, fs, entry) = interrupted(Fault::RenameThenFail(FsErrorKind::Locked));
    let mut vault = open(profile.path(), &fs);
    let settled = purge(&mut vault, entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Restored);
    assert_eq!(settled.target.unwrap().location, fs.location("home/a"));
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Restored);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 7);
}

#[test]
fn a_retried_purge_of_an_interrupted_restore_purges_the_unchanged_payload() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        purge(&mut vault, entry.id).unwrap().state,
        RetainedState::Purged
    );
    assert!(fs.metadata(&held(&entry)).is_err());
}

#[test]
fn a_destination_made_by_someone_else_keeps_both_files_and_needs_attention() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    fs.add_file("home/a", 33);
    let mut vault = open(profile.path(), &fs);
    assert!(restore(&mut vault, entry.id, &fs.location("home/b")).is_err());
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 33);
    assert_eq!(fs.metadata(&held(&entry)).unwrap().size, 7);
    assert!(fs.metadata(&fs.location("home/b")).is_err());
}

#[test]
fn a_missing_payload_after_an_interrupted_restore_needs_attention_and_creates_nothing() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    fs.remove(&held(&entry).path().to_text());
    let mut vault = open(profile.path(), &fs);
    assert!(restore(&mut vault, entry.id, &fs.location("home/b")).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert!(fs.metadata(&fs.location("home/b")).is_err());
}

#[test]
fn an_interrupted_restore_whose_destination_folder_is_gone_returns_to_ready() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.add_folder("home/sub");
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    fail_next_rename(&fs, &entry, Fault::Fail(FsErrorKind::Locked));
    assert!(restore(&mut vault, entry.id, &fs.location("home/sub/a")).is_err());
    fs.remove("home/sub");
    let target = fs.location("home/a");
    assert_eq!(
        restore(&mut vault, entry.id, &target).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.metadata(&target).unwrap().size, 7);
}

/// The maintenance pass a minute after start, before anything has expired.
fn startup_pass(profile: &Path, fs: &FakeFs) -> RetainedState {
    let mut vault = open(profile, fs);
    let at = fs.location("home");
    run(|run| {
        vault.maintain_volume(
            &MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 100,
                max_purges: 8,
            },
            run,
        )
    })
    .unwrap();
    vault.get(RetentionId(1)).unwrap().state
}

#[test]
fn a_changed_payload_needs_attention_until_it_is_back_and_then_settles_at_startup() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let payload = held(&entry).path().to_text();
    fs.rename_entry(&payload, "home/moved");
    fs.add_file(&payload, 9);
    let mut vault = open(profile.path(), &fs);
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    drop(vault);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Conflict);
    assert_eq!(
        fs.metadata(&held(&entry)).unwrap().size,
        9,
        "nothing is moved"
    );
    fs.remove(&payload);
    fs.rename_entry("home/moved", &payload);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Ready);
    let mut vault = open(profile.path(), &fs);
    let target = fs.location("home/a");
    assert_eq!(
        restore(&mut vault, entry.id, &target).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(fs.metadata(&target).unwrap().size, 7);
}

#[test]
fn a_restore_that_landed_before_its_proof_failed_settles_as_restored_at_startup() {
    let (profile, fs, entry) = interrupted(Fault::RenameThenFail(FsErrorKind::Locked));
    let container = entry.record.container.path().to_text();
    let serial = fs.volume_serial(&entry.record.container).unwrap();
    fs.set_volume_serial(&container, None);
    let mut vault = open(profile.path(), &fs);
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    drop(vault);
    fs.set_volume_serial(&container, serial);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Restored);
    assert_eq!(fs.metadata(&fs.location("home/a")).unwrap().size, 7);
}

#[test]
fn an_unsupported_proof_after_an_interrupted_restore_needs_attention() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    fs.set_volume_serial(&entry.record.container.path().to_text(), None);
    let mut vault = open(profile.path(), &fs);
    assert!(restore(&mut vault, entry.id, &fs.location("home/a")).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    drop(vault);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Conflict);
    assert_eq!(fs.metadata(&held(&entry)).unwrap().size, 7);
}

#[test]
fn a_missing_holding_folder_needs_attention_and_settles_once_it_is_back() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let container = entry.record.container.path().to_text();
    fs.rename_entry(&container, "away");
    let mut vault = open(profile.path(), &fs);
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    drop(vault);
    fs.rename_entry("away", &container);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Ready);
    assert_eq!(fs.metadata(&held(&entry)).unwrap().size, 7);
}

#[test]
fn a_missing_anchor_folder_needs_attention_and_settles_once_it_is_back() {
    let (profile, fs, entry) = interrupted(Fault::Fail(FsErrorKind::Locked));
    let anchor = entry.record.anchor().unwrap().path().to_text();
    fs.rename_entry(&anchor, "/away");
    let mut vault = open(profile.path(), &fs);
    assert!(restore(&mut vault, entry.id, &fs.location("home/a")).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Conflict);
    drop(vault);
    fs.rename_entry("/away", &anchor);
    assert_eq!(startup_pass(profile.path(), &fs), RetainedState::Ready);
    assert_eq!(fs.metadata(&held(&entry)).unwrap().size, 7);
}

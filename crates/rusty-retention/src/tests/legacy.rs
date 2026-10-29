use rusty_core::retention::RetentionRecord;

use super::*;

fn as_sibling(profile: &Path, fs: &FakeFs, entry: &RetentionEntry) -> RetentionRecord {
    let name = entry.record.container.name().unwrap().to_text();
    let sibling = format!("home/docs/{name}");
    fs.rename_entry(&entry.record.container.path().to_text(), &sibling);
    let mut record = entry.record.clone();
    record.version = 1;
    record.container = fs.location(&sibling);
    record.parent_identity = fs
        .file_identity(&fs.location("home/docs"))
        .unwrap()
        .unwrap();
    record.validate().unwrap();
    let (db, _) = Db::open(&profile.join("ferrite.db"), false).unwrap();
    db.connection()
        .execute(
            "UPDATE retained_entry SET record = ?2 WHERE id = ?1",
            (entry.id.0, serde_json::to_vec(&record).unwrap()),
        )
        .unwrap();
    record
}

fn legacy(profile: &Path, fs: &FakeFs) -> (RetentionId, RetentionRecord) {
    fs.add_file("home/docs/a", 7);
    fs.set_available_space("home/docs", Some(u64::MAX));
    let mut vault = open(profile, fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    (entry.id, as_sibling(profile, fs, &entry))
}

#[test]
fn an_old_sibling_container_still_purges() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (id, record) = legacy(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(purge(&mut vault, id).unwrap().state, RetainedState::Purged);
    assert!(fs.metadata(&record.container).is_err());
}

#[test]
fn an_old_sibling_capture_cut_short_settles_as_retained() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (id, record) = legacy(profile.path(), &fs);
    raw_state(profile.path(), id, "preparing");
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, id).unwrap().state,
        RetainedState::Ready
    );
    assert!(fs.metadata(&record.held().unwrap()).is_ok());
}

#[test]
fn an_old_sibling_restore_cut_short_settles_as_restored() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (id, record) = legacy(profile.path(), &fs);
    let mut vault = open(profile.path(), &fs);
    begin_restore(&mut vault, id, &record.source);
    fs.rename_atomic(&record.held().unwrap(), &record.source)
        .unwrap();
    assert_eq!(
        reconcile(&mut vault, id).unwrap().state,
        RetainedState::Restored
    );
    assert!(fs.metadata(&record.container).is_err());
    assert_eq!(fs.metadata(&record.source).unwrap().size, 7);
}

#[test]
fn maintenance_cleans_an_old_sibling_container_left_after_purge() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let (id, record) = legacy(profile.path(), &fs);
    fs.remove_file(&record.held().unwrap()).unwrap();
    raw_state(profile.path(), id, "purged");
    let mut vault = open(profile.path(), &fs);
    let at = fs.location("home/docs");
    let report = run(|run| {
        vault.maintain_volume(
            MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 100,
                max_purges: 4,
            },
            run,
        )
    })
    .unwrap();
    assert!(!report.cleanup_pending);
    assert!(fs.metadata(&record.container).is_err());
}

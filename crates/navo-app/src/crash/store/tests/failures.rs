use super::*;

#[test]
fn a_future_database_version_is_never_quarantined() {
    let data = tempfile::tempdir().unwrap();
    let path = database(data.path());
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .pragma_update(None, "user_version", Db::VERSION + 1)
        .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(super::super::super::open_db(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!data.path().join("navo.db.corrupt-1").exists());
}

#[test]
fn a_shared_session_never_quarantines_a_live_database() {
    let data = tempfile::tempdir().unwrap();
    let mut first = check_with(Some(data.path()), false, &FakeFs::new(), true);
    first.share();
    let path = database(data.path());
    fs::write(super::super::super::sentinel_path(&path), b"").unwrap();
    let before = fs::read(&path).unwrap();
    let second = check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(second.notice.is_some());
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(!data.path().join("navo.db.corrupt-1").exists());
}

#[test]
fn malformed_or_unsupported_recovery_records_are_kept_intact() {
    for bytes in [
        b"{".as_slice(),
        br#"{"version":2,"next_case":1,"pending":null,"notices":[]}"#,
    ] {
        let data = tempfile::tempdir().unwrap();
        fs::write(State::path(data.path()), bytes).unwrap();
        check_with(Some(data.path()), true, &FakeFs::new(), true);
        assert_eq!(fs::read(State::path(data.path())).unwrap(), bytes);
        assert!(!database(data.path()).exists());
    }
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_database_is_never_opened_or_moved() {
    let data = tempfile::tempdir().unwrap();
    let outside = data.path().join("other.db");
    let (db, _) = Db::open(&outside, false).unwrap();
    drop(db);
    let before = fs::read(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, database(data.path())).unwrap();
    check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert_eq!(fs::read(outside).unwrap(), before);
    assert!(database(data.path()).is_symlink());
}

#[cfg(unix)]
#[test]
fn a_failed_sentinel_write_still_blocks_every_database_consumer_for_the_run() {
    use std::os::unix::fs::PermissionsExt;

    let data = tempfile::tempdir().unwrap();
    let mut first = check_with(Some(data.path()), false, &FakeFs::new(), true);
    first.share();
    drop(first);
    fs::write(State::path(data.path()), b"malformed record").unwrap();
    let permissions = fs::metadata(data.path()).unwrap().permissions();
    fs::set_permissions(data.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let start = check_with(Some(data.path()), true, &FakeFs::new(), true);
    fs::set_permissions(data.path(), permissions).unwrap();
    let path = database(data.path());
    assert!(!crate::crash::sentinel_path(&path).exists());
    assert!(crate::crash::open_db(&path).is_err());
    assert!(crate::jobs::open(&path).is_err());
    assert!(crate::files::open_memory(&path).is_none());
    assert!(start.notice.is_some());
}

#[test]
fn a_pending_case_cannot_also_be_a_completed_notice() {
    let data = tempfile::tempdir().unwrap();
    let mut state = built(data.path());
    let Some(Progress::Promote { notice, .. }) = state.pending.clone() else {
        panic!("built candidate");
    };
    state.notices.push(notice);
    let bytes = serde_json::to_vec(&state).unwrap();
    fs::write(State::path(data.path()), &bytes).unwrap();
    check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert_eq!(fs::read(State::path(data.path())).unwrap(), bytes);
    assert!(!database(data.path()).exists());
}

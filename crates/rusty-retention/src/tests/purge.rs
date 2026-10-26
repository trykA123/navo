use super::*;

#[test]
fn purge_removes_only_the_captured_tree_and_keeps_a_new_original() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    fs.add_file("home/tree/nested/b", 11);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    fs.add_file("home/tree/new", 33);
    assert_eq!(
        purge(&mut vault, entry.id).unwrap().state,
        RetainedState::Purged
    );
    assert!(fs.metadata(&entry.record.container).is_err());
    assert_eq!(fs.metadata(&fs.location("home/tree/new")).unwrap().size, 33);
}

#[test]
fn unexpected_children_stop_purge_before_any_captured_file_is_removed() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    let held = entry.record.held().unwrap();
    let extra = held.join(&ProviderPath::from_text("new")).unwrap();
    fs.add_file(extra.path().to_text().trim_start_matches('/'), 33);
    assert!(purge(&mut vault, entry.id).is_err());
    assert!(fs.metadata(&extra).is_ok());
    assert!(
        fs.metadata(&held.join(&ProviderPath::from_text("a")).unwrap())
            .is_ok()
    );
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
}

#[test]
fn a_replaced_payload_is_not_purged_or_restored() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    let held = entry.record.held().unwrap();
    fs.remove_file(&held).unwrap();
    fs.add_file(held.path().to_text().trim_start_matches('/'), 7);
    assert!(purge(&mut vault, entry.id).is_err());
    assert!(restore(&mut vault, entry.id, &fs.location("home/a")).is_err());
    assert!(fs.metadata(&held).is_ok());
}

#[test]
fn an_unavailable_parent_does_not_mean_the_retained_data_was_deleted() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    fs.fault(
        "home",
        FakeOp::Metadata,
        fs.calls("home", FakeOp::Metadata) + 1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
}

#[test]
fn a_partial_purge_resumes_from_the_persisted_manifest() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    fs.add_file("home/tree/b", 11);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    let b = entry
        .record
        .held()
        .unwrap()
        .join(&ProviderPath::from_text("b"))
        .unwrap();
    fs.fault(
        b.path().to_text().trim_start_matches('/'),
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Purging);
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Purged
    );
}

#[test]
fn purge_resumes_after_an_entire_recorded_subfolder_was_already_removed() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/sub/a", 7);
    fs.add_file("home/tree/b", 11);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    let held = entry.record.held().unwrap();
    let b = held.join(&ProviderPath::from_text("b")).unwrap();
    fs.fault(
        b.path().to_text().trim_start_matches('/'),
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    assert!(purge(&mut vault, entry.id).is_err());
    assert!(
        fs.metadata(&held.join(&ProviderPath::from_text("sub")).unwrap())
            .is_err()
    );
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Purged
    );
}

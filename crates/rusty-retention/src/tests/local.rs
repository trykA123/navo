use super::*;

#[test]
fn actual_local_bytes_and_file_identity_survive_capture_and_reopen_restore() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("sample.bin");
    let bytes = b"retained original bytes\0\xff";
    std::fs::write(&path, bytes).unwrap();
    let fs = LocalFs::new();
    let source = location(&path);
    let before = fs.file_identity(&source).unwrap();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &source);
    assert_eq!(
        fs.file_identity(&entry.record.held().unwrap()).unwrap(),
        before
    );
    assert!(!path.exists());
    drop(vault);
    let mut vault = open_within(profile.path(), &fs, files.path());
    restore(&mut vault, entry.id, &source).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(fs.file_identity(&source).unwrap(), before);
}

#[test]
fn actual_nested_directories_are_removed_only_by_explicit_purge() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let tree = files.path().join("tree");
    std::fs::create_dir_all(tree.join("nested")).unwrap();
    std::fs::write(tree.join("nested/a"), b"old").unwrap();
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &location(&tree));
    let held = Path::new(entry.record.container.as_str()).join("payload");
    assert_eq!(std::fs::read(held.join("nested/a")).unwrap(), b"old");
    purge(&mut vault, entry.id).unwrap();
    assert!(!held.exists());
}

#[cfg(unix)]
#[test]
fn retained_symlinks_are_never_followed_during_capture_or_purge() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let target = files.path().join("target");
    std::fs::write(&target, b"keep outside target").unwrap();
    let tree = files.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::os::unix::fs::symlink(&target, tree.join("link")).unwrap();
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &location(&tree));
    purge(&mut vault, entry.id).unwrap();
    assert_eq!(std::fs::read(target).unwrap(), b"keep outside target");
}

#[test]
fn an_in_place_payload_edit_prevents_automatic_purge() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("sample");
    std::fs::write(&path, b"old").unwrap();
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &location(&path));
    let held = Path::new(entry.record.container.as_str()).join("payload");
    std::fs::write(&held, b"modified retained bytes").unwrap();
    assert!(purge(&mut vault, entry.id).is_err());
    assert_eq!(std::fs::read(held).unwrap(), b"modified retained bytes");
}

#[test]
fn a_restored_file_edited_before_notice_commit_is_still_recognized_by_identity() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("sample");
    std::fs::write(&path, b"old").unwrap();
    let source = location(&path);
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &source);
    begin_restore(&mut vault, entry.id, &source);
    fs.rename(&entry.record.held().unwrap(), &source).unwrap();
    std::fs::write(&path, b"new user edits").unwrap();
    assert_eq!(
        reconcile(&mut vault, entry.id).unwrap().state,
        RetainedState::Restored
    );
    assert_eq!(std::fs::read(path).unwrap(), b"new user edits");
}

#[cfg(unix)]
#[test]
fn a_parent_replaced_with_a_link_during_purge_cannot_delete_the_moved_tree() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let tree = files.path().join("tree");
    let outside = files.path().join("outside");
    std::fs::create_dir_all(tree.join("sub")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(tree.join("sub/a"), b"keep moved contents").unwrap();
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &location(&tree));
    let held = Path::new(entry.record.container.as_str()).join("payload");
    let child = location(&held.join("sub/a"));
    let mut visits = 0;
    let mut progress = |progress: RetentionProgress| {
        if progress.current == child {
            visits += 1;
            if visits == 3 {
                std::fs::rename(held.join("sub"), outside.join("sub")).unwrap();
                std::os::unix::fs::symlink(outside.join("sub"), held.join("sub")).unwrap();
            }
        }
    };
    let cancel = CancelToken::new();
    assert!(
        vault
            .purge(entry.id, &mut RetentionRun::new(&cancel, &mut progress))
            .is_err()
    );
    assert_eq!(
        std::fs::read(outside.join("sub/a")).unwrap(),
        b"keep moved contents"
    );
}

#[test]
fn a_target_created_after_restore_preflight_is_never_overwritten() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("sample");
    std::fs::write(&path, b"old").unwrap();
    let source = location(&path);
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &source);
    let mut visits = 0;
    let mut progress = |progress: RetentionProgress| {
        if progress.current == source {
            visits += 1;
            if visits == 2 {
                std::fs::write(&path, b"new target").unwrap();
            }
        }
    };
    let cancel = CancelToken::new();
    assert!(
        vault
            .restore(
                RestoreRequest {
                    id: entry.id,
                    target: &source
                },
                &mut RetentionRun::new(&cancel, &mut progress)
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"new target");
    let held = Path::new(entry.record.container.as_str()).join("payload");
    assert_eq!(std::fs::read(held).unwrap(), b"old");
}

#[test]
fn a_payload_replaced_after_restore_preflight_stays_in_its_original_location() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("sample");
    std::fs::write(&path, b"old").unwrap();
    let source = location(&path);
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &source);
    let held = Path::new(entry.record.container.as_str()).join("payload");
    let moved = files.path().join("moved-original");
    let mut visits = 0;
    let mut progress = |progress: RetentionProgress| {
        if progress.current == source {
            visits += 1;
            if visits == 2 {
                std::fs::rename(&held, &moved).unwrap();
                std::fs::write(&held, b"replacement payload").unwrap();
            }
        }
    };
    let cancel = CancelToken::new();
    assert!(
        vault
            .restore(
                RestoreRequest {
                    id: entry.id,
                    target: &source
                },
                &mut RetentionRun::new(&cancel, &mut progress)
            )
            .is_err()
    );
    assert!(!path.exists());
    assert_eq!(std::fs::read(held).unwrap(), b"replacement payload");
    assert_eq!(std::fs::read(moved).unwrap(), b"old");
}

#[test]
fn a_native_capture_interrupted_after_rename_reconciles_after_reopening() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let tree = files.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::write(tree.join("a"), b"original interrupted bytes").unwrap();
    let fs = LocalFs::new();
    let source = location(&tree);
    let original = fs.file_identity(&source).unwrap();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let mut progress = |progress: RetentionProgress| {
        if progress
            .current
            .name()
            .is_some_and(|name| name.to_text() == "payload")
        {
            panic!("simulated interruption after native rename");
        }
    };
    let cancel = CancelToken::new();
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        vault.capture(
            CaptureRequest {
                source: &source,
                policy: Default::default(),
                created_at: 0,
                job: None,
            },
            &mut RetentionRun::new(&cancel, &mut progress),
        )
    }));
    assert!(interrupted.is_err());
    let entry = vault.list_after(0, 1).unwrap().remove(0);
    assert_eq!(entry.state, RetainedState::Preparing);
    assert!(!tree.exists());
    drop(vault);
    let mut vault = open_within(profile.path(), &fs, files.path());
    let ready = reconcile(&mut vault, entry.id).unwrap();
    assert_eq!(ready.state, RetainedState::Ready);
    assert_eq!(
        fs.file_identity(&ready.record.held().unwrap()).unwrap(),
        original
    );
    restore(&mut vault, ready.id, &source).unwrap();
    assert_eq!(
        std::fs::read(tree.join("a")).unwrap(),
        b"original interrupted bytes"
    );
}

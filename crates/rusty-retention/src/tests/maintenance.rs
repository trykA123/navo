use super::*;

fn request(at: &Location, now: i64, policy: RetentionPolicy) -> MaintenanceRequest<'_> {
    MaintenanceRequest {
        at,
        now,
        policy,
        max_purges: 256,
    }
}

fn pair(vault: &mut Vault<'_>, fs: &FakeFs) -> Vec<RetentionEntry> {
    ["home/a", "home/b"]
        .into_iter()
        .map(|name| {
            fs.add_file(name, 7);
            capture(vault, &fs.location(name))
        })
        .collect()
}

#[test]
fn maintenance_expiry_budget_reports_actual_remaining_bytes_and_resumes_next_pass() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(1000));
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let at = fs.location("home");
    let mut args = request(&at, 4 * DAY_MS, RetentionPolicy::new(3, 1, 0).unwrap());
    args.max_purges = 1;
    let result = run(|run| vault.maintain_volume(args, run)).unwrap();
    assert_eq!(result.purged, vec![entries[0].id]);
    assert_eq!(result.remaining_bytes, 7);
    assert!(result.budget_exhausted);
    assert!(result.quota_unmet);
    let result =
        run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
            .unwrap();
    assert_eq!(result.purged, vec![entries[1].id]);
    assert_eq!(result.remaining_bytes, 0);
    assert!(!result.budget_exhausted);
}

#[test]
fn maintenance_rechecks_physical_space_after_each_purge_and_stops_when_recovered() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(0));
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let first = entries[0].record.held().unwrap();
    let at = fs.location("home");
    let result = vault
        .maintain_volume(
            request(&at, 100, RetentionPolicy::new(3, 1000, 10).unwrap()),
            &mut RetentionRun::new(&CancelToken::new(), &mut |update| {
                if update.current == first {
                    fs.set_available_space("home", Some(100));
                }
            }),
        )
        .unwrap();
    assert_eq!(result.purged, vec![entries[0].id]);
    assert!(!result.low_space);
    assert_eq!(fs.calls("home", FakeOp::Space), 2);
    assert!(fs.metadata(&entries[1].record.held().unwrap()).is_ok());
}

#[test]
fn maintenance_quota_uses_actual_remaining_logical_inventory() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(1000));
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let at = fs.location("home");
    let result = run(|run| {
        vault.maintain_volume(
            request(&at, 100, RetentionPolicy::new(3, 8, 0).unwrap()),
            run,
        )
    })
    .unwrap();
    assert_eq!(result.purged, vec![entries[0].id]);
    assert_eq!(result.remaining_bytes, 7);
    assert!(!result.quota_unmet);
    assert_eq!(
        vault.get(entries[1].id).unwrap().state,
        RetainedState::Ready
    );
}

#[test]
fn unavailable_space_observation_refuses_any_purge() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let at = fs.location("home");
    assert!(
        run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
            .is_err()
    );
    for entry in entries {
        assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
    }
}

#[test]
fn failed_fresh_space_query_stops_after_the_already_committed_purge() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(0));
    fs.fault(
        "home",
        FakeOp::Space,
        2,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let at = fs.location("home");
    assert!(
        run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
            .is_err()
    );
    assert_eq!(
        vault.get(entries[0].id).unwrap().state,
        RetainedState::Purged
    );
    assert_eq!(
        vault.get(entries[1].id).unwrap().state,
        RetainedState::Ready
    );
    assert!(fs.metadata(&entries[1].record.held().unwrap()).is_ok());
}

#[test]
fn maintenance_resumes_an_interrupted_purge_before_choosing_new_entries() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/tree/a", 7);
    fs.add_file("home/tree/b", 11);
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/tree"));
    let held = entry
        .record
        .held()
        .unwrap()
        .join(&ProviderPath::from_text("b"))
        .unwrap();
    fs.fault(
        &held.path().to_text(),
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    assert!(purge(&mut vault, entry.id).is_err());
    drop(vault);
    let mut vault = open(profile.path(), &fs);
    let at = fs.location("home");
    let result =
        run(|run| vault.maintain_volume(request(&at, 100, RetentionPolicy::default()), run))
            .unwrap();
    assert_eq!(result.purged, vec![entry.id]);
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Purged);
    assert!(fs.metadata(&entry.record.container).is_err());
}

#[test]
fn changed_observed_mount_and_cancelled_pass_preserve_payloads() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(0));
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    let at = fs.location("home");
    let mut calls = 0;
    let result = vault.maintain_volume(
        request(&at, 4 * DAY_MS, RetentionPolicy::default()),
        &mut RetentionRun::new(&CancelToken::new(), &mut |_| {
            calls += 1;
            if calls == 2 {
                fs.set_mount_identity("home", Some(99));
            }
        }),
    );
    assert!(result.is_err());
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(
        vault
            .maintain_volume(
                request(&at, 4 * DAY_MS, RetentionPolicy::default()),
                &mut RetentionRun::new(&cancel, &mut |_| {})
            )
            .is_err()
    );
    for entry in entries {
        assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
    }
}

#[test]
fn native_maintenance_removes_expired_payload_and_preserves_new_user_file() {
    let profile = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("a");
    std::fs::write(&source, b"old version").unwrap();
    let fs = LocalFs::new();
    let mut vault = open_within(profile.path(), &fs, files.path());
    let entry = capture(&mut vault, &location(&source));
    std::fs::write(&source, b"new user version").unwrap();
    let at = location(files.path());
    let result =
        run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
            .unwrap();
    assert_eq!(result.purged, vec![entry.id]);
    assert_eq!(std::fs::read(source).unwrap(), b"new user version");
    assert!(fs.metadata(&entry.record.container).is_err());
}

#[test]
fn maintenance_preserves_pinned_versions_and_reports_unmet_limits() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(0));
    let mut vault = open(profile.path(), &fs);
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    let mut journal = rusty_store::JobJournal::new(db).unwrap();
    let job = journal.create_job("copy", b"{}", 0, 100).unwrap();
    let entry = run(|run| {
        vault.capture(
            CaptureRequest {
                source: &fs.location("home/a"),
                policy: RetentionPolicy::default(),
                created_at: 100,
                job: Some(job),
            },
            run,
        )
    })
    .unwrap();
    let at = fs.location("home");
    let report = run(|run| {
        vault.maintain_volume(
            request(&at, 4 * DAY_MS, RetentionPolicy::new(3, 1, 10).unwrap()),
            run,
        )
    })
    .unwrap();
    assert!(report.purged.is_empty());
    assert!(report.quota_unmet);
    assert!(report.low_space);
    assert_eq!(report.remaining_bytes, 7);
    assert!(vault.get(entry.id).unwrap().pinned);
    assert!(fs.metadata(&entry.record.held().unwrap()).is_ok());
}

#[test]
fn maintenance_retries_empty_holder_cleanup_after_the_purge_state_was_committed() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    fs.fault(
        &entry.record.container.path().to_text(),
        FakeOp::Remove,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let at = fs.location("home");
    let first =
        run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
            .unwrap();
    assert!(first.cleanup_pending);
    assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Purged);
    assert!(fs.metadata(&entry.record.container).is_ok());
    run(|run| vault.maintain_volume(request(&at, 4 * DAY_MS, RetentionPolicy::default()), run))
        .unwrap();
    assert!(fs.metadata(&entry.record.container).is_err());
}

#[test]
fn a_failed_cleanup_does_not_starve_later_empty_holders_on_the_next_pass() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let entries = pair(&mut vault, &fs);
    for entry in &entries {
        fs.remove_file(&entry.record.held().unwrap()).unwrap();
        raw_state(profile.path(), entry.id, "purged");
    }
    let extra = entries[0]
        .record
        .container
        .join(&ProviderPath::from_text("extra"))
        .unwrap();
    fs.add_file(&extra.path().to_text(), 9);
    let at = fs.location("home");
    let args = || MaintenanceRequest {
        max_purges: 1,
        ..request(&at, 100, RetentionPolicy::default())
    };
    run(|run| vault.maintain_volume(args(), run)).unwrap();
    let result = run(|run| vault.maintain_volume(args(), run)).unwrap();
    assert!(result.cleanup_pending);
    assert!(fs.metadata(&extra).is_ok());
    assert!(fs.metadata(&entries[1].record.container).is_err());
}

#[test]
fn terminal_cleanup_without_container_identity_keeps_an_unknown_holder_pending() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    let mut record = entry.record.clone();
    record.container_identity = None;
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    db.connection().execute("UPDATE retained_entry SET state = 'aborted', record = CAST(json_set(record, '$.container_identity', NULL) AS BLOB) WHERE id = ?1", [entry.id.0]).unwrap();
    let at = fs.location("home");
    let result =
        run(|run| vault.maintain_volume(request(&at, 100, RetentionPolicy::default()), run))
            .unwrap();
    assert!(result.cleanup_pending);
    assert!(fs.metadata(&record.held().unwrap()).is_ok());
    fs.rename_atomic(&record.held().unwrap(), &record.source)
        .unwrap();
    fs.remove_empty_folder(&record.container).unwrap();
    let result =
        run(|run| vault.maintain_volume(request(&at, 100, RetentionPolicy::default()), run))
            .unwrap();
    assert!(!result.cleanup_pending);
    assert_eq!(fs.metadata(&record.source).unwrap().size, 7);
}

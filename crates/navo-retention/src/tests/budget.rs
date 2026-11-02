use super::*;

fn stick(fs: &FakeFs, entry: &RetentionEntry) {
    let container = entry.record.container.path().to_text();
    let seen = fs.calls(&container, FakeOp::Identity);
    for call in 1..=64 {
        fs.fault(
            &container,
            FakeOp::Identity,
            seen + call,
            Fault::Fail(FsErrorKind::NetworkLost),
        );
    }
}

#[test]
fn stuck_entries_use_up_the_purge_budget_of_one_pass() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let stuck: Vec<_> = (0..4)
        .map(|index| {
            let name = format!("home/{index}");
            fs.add_file(&name, 7);
            capture(&mut vault, &fs.location(&name))
        })
        .collect();
    for entry in &stuck {
        stick(&fs, entry);
    }
    let at = fs.location("home");
    let report = run(|run| {
        vault.maintain_volume(
            &MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 4 * DAY_MS,
                max_purges: 2,
            },
            run,
        )
    })
    .unwrap();
    assert!(report.purged.is_empty());
    assert_eq!(report.skipped, [stuck[0].id, stuck[1].id]);
    assert!(report.budget_exhausted);
}

fn maintain(
    vault: &mut Vault<'_>,
    at: &Location,
    policy: RetentionPolicy,
    max_purges: u16,
) -> MaintenanceReport {
    run(|run| {
        vault.maintain_volume(
            &MaintenanceRequest {
                at,
                policy,
                now: 4 * DAY_MS,
                max_purges,
            },
            run,
        )
    })
    .unwrap()
}

#[test]
fn a_stuck_entry_keeps_its_bytes_and_the_unmet_quota_in_the_report() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    fs.add_file("home/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));
    stick(&fs, &entry);
    let policy = RetentionPolicy::new(7, 1, 0).unwrap();
    let report = maintain(&mut vault, &fs.location("home"), policy, 8);
    assert_eq!(report.skipped, [entry.id]);
    assert_eq!(report.remaining_bytes, 7);
    assert!(report.quota_unmet);
    assert!(!report.budget_exhausted);
}

#[test]
fn stuck_cleanups_use_up_the_budget_shared_with_purges() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    for name in ["home/a", "home/b"] {
        fs.add_file(name, 7);
        let entry = capture(&mut vault, &fs.location(name));
        fs.remove_file(&entry.record.held().unwrap()).unwrap();
        raw_state(profile.path(), entry.id, "purged");
        for call in 1..=8 {
            fs.fault(
                &entry.record.container.path().to_text(),
                FakeOp::Remove,
                call,
                Fault::Fail(FsErrorKind::Locked),
            );
        }
    }
    fs.add_file("home/c", 7);
    let ready = capture(&mut vault, &fs.location("home/c"));
    let report = maintain(
        &mut vault,
        &fs.location("home"),
        RetentionPolicy::default(),
        2,
    );
    assert_eq!(report.skipped.len(), 2);
    assert!(report.purged.is_empty());
    assert!(report.budget_exhausted);
    assert_eq!(vault.get(ready.id).unwrap().state, RetainedState::Ready);
}

#[test]
fn a_cancel_during_a_skipped_entry_stops_the_pass() {
    let cancel = CancelToken::new();
    let mut progress = |_| {};
    let run = RetentionRun::new(&cancel, &mut progress);
    let id = RetentionId(1);
    assert!(crate::maintenance::skip(id, Error::State(id), &run).is_ok());
    cancel.cancel();
    assert!(crate::maintenance::skip(id, Error::State(id), &run).is_err());
}

use super::*;

fn pass(vault: &mut Vault<'_>, at: &Location) -> MaintenanceReport {
    run(|run| {
        vault.maintain_volume(
            &MaintenanceRequest {
                at,
                policy: RetentionPolicy::default(),
                now: 4 * DAY_MS,
                max_purges: 256,
            },
            run,
        )
    })
    .unwrap()
}

#[test]
fn healthy_expired_entries_are_reached_behind_three_hundred_failing_ones() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let mut capture_file = |name: String| {
        fs.add_file(&name, 7);
        capture(&mut vault, &fs.location(&name))
    };
    let failing: Vec<_> = (0..300)
        .map(|i| capture_file(format!("home/f{i}")))
        .collect();
    let healthy: Vec<_> = (0..5).map(|i| capture_file(format!("home/h{i}"))).collect();
    for entry in &failing {
        fs.add_file(&entry.record.held().unwrap().path().to_text(), 8);
    }
    let at = fs.location("home");
    let first = pass(&mut vault, &at);
    assert_eq!(first.skipped.len(), 256);
    let second = pass(&mut vault, &at);
    let mut purged = second.purged.clone();
    purged.sort_by_key(|id| id.0);
    let expected: Vec<_> = healthy.iter().map(|entry| entry.id).collect();
    assert_eq!(purged, expected);
    assert_eq!(second.remaining_bytes, 300 * 7);
    for entry in &failing {
        assert_eq!(vault.get(entry.id).unwrap().state, RetainedState::Ready);
        assert_eq!(fs.metadata(&entry.record.held().unwrap()).unwrap().size, 8);
    }
}

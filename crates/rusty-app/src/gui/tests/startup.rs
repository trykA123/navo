use super::*;

#[test]
fn startup_journal_connection_count() {
    for retention in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let dirs = store(&root);
        for unclean in [false, false, true] {
            check_startup_connections(&dirs, retention, unclean);
        }
    }
}

fn check_startup_connections(dirs: &StoreDirs, retention: bool, unclean: bool) {
    let before = rusty_store::test_support::connection_opens(&dirs.journal_file());
    let start =
        crate::crash::store::check_with(Some(&dirs.data), unclean, &FakeFs::new(), retention);
    let settings = crate::settings::start_from((Some(dirs), start.journal.clone()), retention);
    let local = std::sync::Arc::new(rusty_fs::LocalFs::new()) as std::sync::Arc<dyn ProviderOps>;
    let services = super::super::profile_services(
        (Some(dirs), start.journal.clone()),
        &home(),
        retention,
        &local,
    );
    assert!(settings.finish().loaded.is_some());
    assert!(std::sync::Arc::ptr_eq(
        start.journal.as_ref().unwrap(),
        services.memory.as_ref().unwrap()
    ));
    services.maintenance.stop();
    assert_eq!(
        rusty_store::test_support::connection_opens(&dirs.journal_file()) - before,
        1
    );
}

#[test]
fn quit_checkpoints_the_shared_journal_after_flushing_visits() {
    let root = tempfile::tempdir().unwrap();
    let dirs = store(&root);
    let start = crate::crash::store::check_with(Some(&dirs.data), false, &FakeFs::new(), false);
    let journal = start.journal.clone().unwrap();
    let auto: u32 = jobs::lock(&journal)
        .database()
        .connection()
        .pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
        .unwrap();
    assert_eq!(auto, 1000);
    jobs::lock(&journal)
        .create_job("copy", &vec![0; 5 * 1024 * 1024], 0, 0)
        .unwrap();
    let wal = dirs.data.join("navo.db-wal");
    assert!(std::fs::metadata(&wal).unwrap().len() > 5 * 1024 * 1024);
    let (search, visits) = crate::search_hooks::with_visits(
        Some(journal.clone()),
        crate::search_hooks::Remember::default(),
    );
    assert!(visits.is_some());
    (search.record_visit)("/pending", true);
    let quit = super::super::quitter(
        (None, visits, Some(journal.clone())),
        crate::profile::Cleanup::default(),
        None,
        crate::jobs::maintenance::MaintenanceTimer::default(),
    );
    quit();
    assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);
    assert_eq!(
        jobs::lock(&journal)
            .database()
            .frecent(1, crate::files::now_ms())
            .unwrap()[0]
            .location,
        "/pending"
    );
}

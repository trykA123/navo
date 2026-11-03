use navo_core::Location;
use navo_fs::{FakeFs, ProviderOps};
use navo_store::StoreDirs;

use crate::jobs;

fn operations(
    dirs: Option<&StoreDirs>,
    home: &Location,
    ops: &dyn ProviderOps,
) -> (
    navo_ui::browser::Operations,
    Option<crate::crash::store::StoreStart>,
    Option<jobs::SharedJournal>,
) {
    operations_mode(dirs, home, ops, false)
}

fn operations_mode(
    dirs: Option<&StoreDirs>,
    home: &Location,
    ops: &dyn ProviderOps,
    experimental: bool,
) -> (
    navo_ui::browser::Operations,
    Option<crate::crash::store::StoreStart>,
    Option<jobs::SharedJournal>,
) {
    let mut start = crate::crash::store::check_with(
        dirs.map(|dirs| dirs.data.as_path()),
        false,
        ops,
        experimental,
    );
    let (operations, journal) = super::operations(
        (dirs, start.journal.clone()),
        home,
        experimental,
        &(std::sync::Arc::new(navo_fs::LocalFs::new()) as std::sync::Arc<dyn navo_fs::ProviderOps>),
    );
    start.share();
    let journal = journal.as_ref().map(jobs::SessionHolds::journal);
    (operations, dirs.map(|_| start), journal)
}

fn store(dir: &tempfile::TempDir) -> StoreDirs {
    StoreDirs {
        config: dir.path().join("config"),
        data: dir.path().join("data"),
    }
}

fn home() -> Location {
    Location::new("/home/admin").unwrap()
}

fn one_running_job(dirs: &StoreDirs) {
    dirs.ensure().unwrap();
    let journal = jobs::open(&dirs.journal_file()).unwrap();
    journal
        .lock()
        .unwrap()
        .create_job("copy", b"not a plan", 0, 0)
        .unwrap();
}

fn running_jobs(dirs: &StoreDirs) -> usize {
    let journal = jobs::open(&dirs.journal_file()).unwrap();
    let guard = journal.lock().unwrap();
    guard.unfinished().unwrap().len()
}

#[test]
fn without_store_folders_only_home_is_protected() {
    let (ops, lock, _) = operations(None, &home(), &FakeFs::new());
    assert_eq!(ops.protected, [home()]);
    assert!(lock.is_none());
}

#[test]
fn unavailable_journal_and_profile_notices_offer_safe_mode() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    dirs.ensure().unwrap();
    std::fs::create_dir(dirs.journal_file()).unwrap();
    for profile in [None, Some(&dirs)] {
        let (operations, journal) = super::operations(
            (profile, None),
            &home(),
            true,
            &(std::sync::Arc::new(navo_fs::LocalFs::new())
                as std::sync::Arc<dyn navo_fs::ProviderOps>),
        );
        assert!(journal.is_none());
        let plan = navo_core::OperationPlan {
            kind: navo_core::OpKind::Copy,
            steps: Vec::new(),
            totals: navo_core::Totals {
                files: 0,
                folders: 0,
                bytes: 0,
            },
            risks: Vec::new(),
            conflicts: Vec::new(),
        };
        let Err(message) = (operations.log)(&plan) else {
            panic!("file changes must remain blocked");
        };
        assert!(
            message
                .message()
                .ends_with(&navo_l10n::tr("retention-safe-mode-help"))
        );
    }
}

#[test]
fn store_folders_are_protected_and_get_a_journal() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let (ops, lock, _) = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(ops.protected.len(), 3);
    assert!(dirs.journal_file().exists());
    assert!(lock.is_some());
}

#[test]
fn the_first_instance_closes_jobs_left_running() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let _started = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(running_jobs(&dirs), 0);
}

#[test]
fn a_second_instance_leaves_the_first_ones_jobs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let first = operations(Some(&dirs), &home(), &FakeFs::new());
    one_running_job(&dirs);
    let second = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(running_jobs(&dirs), 1);
    drop(second);
    drop(first);
}

#[test]
fn a_later_instance_leaves_a_live_instances_jobs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let first = operations(Some(&dirs), &home(), &FakeFs::new());
    let second = operations(Some(&dirs), &home(), &FakeFs::new());
    one_running_job(&dirs);
    drop(first);
    let _third = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(running_jobs(&dirs), 1);
    drop(second);
}

#[test]
fn pane_folders_refuse_missing_folders() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    for side in ["--left", "--right"] {
        let start = crate::cli::Start {
            left: (side == "--left").then(|| missing.clone()),
            right: (side == "--right").then(|| missing.clone()),
            ..Default::default()
        };
        let error = super::pane_folders(&start).unwrap_err();
        assert!(error.contains(side) && error.contains("folder"), "{error}");
    }
}

#[test]
fn pane_folders_refuse_files() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file.txt");
    std::fs::write(&file, "text").unwrap();
    let start = crate::cli::Start {
        left: Some(file),
        ..Default::default()
    };
    let error = super::pane_folders(&start).unwrap_err();
    assert!(
        error.contains("--left") && error.contains("folder"),
        "{error}"
    );
}

#[cfg(feature = "tester")]
#[test]
fn settings_read_is_marked_after_loading_finishes() {
    let mut completed = None;
    let _settings = super::finish_settings(
        || {
            completed = Some(std::time::Instant::now());
            None
        },
        &crate::settings::Follow {
            remember: crate::search_hooks::Remember::default(),
            git: std::sync::Arc::new(|_| {}),
            auto_updates: std::sync::Arc::new(|_| {}),
        },
    );
    let at = crate::metrics::all()
        .into_iter()
        .find_map(|(name, at)| (name == "settings read").then_some(at))
        .unwrap();
    assert!(
        at >= completed.unwrap(),
        "settings marked before completion"
    );
}

#[test]
fn clean_quitter_removes_the_crash_running_marker() {
    let dir = tempfile::tempdir().unwrap();
    let running = crate::crash::Running::start(dir.path()).unwrap();
    let quit = super::quitter(
        (None, None, None),
        crate::profile::Cleanup::default(),
        Some(running),
        crate::jobs::maintenance::MaintenanceTimer::default(),
    );
    quit();
    assert!(!dir.path().join("running").exists());
    assert!(crate::crash::pending(dir.path()).is_none());
}

#[test]
fn a_recorded_failed_integrity_check_preserves_the_original_and_opens_a_fresh_journal() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let db = dirs.journal_file();
    crate::crash::test_damage::fill_history(&db);
    crate::crash::test_damage::corrupt_last_page(&db);
    let bytes = std::fs::read(&db).unwrap();
    let sentinel = crate::crash::sentinel_path(&db);
    std::fs::write(&sentinel, b"").unwrap();
    let (ops, lock, _) = operations_mode(Some(&dirs), &home(), &FakeFs::new(), true);
    assert_eq!(ops.protected.len(), 3);
    drop((ops, lock));
    assert!(!sentinel.exists());
    assert_eq!(
        std::fs::read(dirs.data.join("navo.db.corrupt-1")).unwrap(),
        bytes
    );
    let (opened, _) = navo_store::Db::open(&db, false).unwrap();
    let journal = navo_store::JobJournal::new(opened).unwrap();
    assert_eq!(journal.unfinished().unwrap().len(), 0);
}

#[test]
fn a_corrupt_journal_found_during_recovery_is_put_on_record() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let db = dirs.journal_file();
    let page = crate::crash::test_damage::root_page(&db, "job");
    crate::crash::test_damage::corrupt_page(&db, page);
    let _started = operations_mode(Some(&dirs), &home(), &FakeFs::new(), true);
    assert!(crate::crash::sentinel_path(&db).exists());
}

#[test]
fn delete_confirmation_retention_requires_the_flag_and_the_current_setting() {
    let root = tempfile::tempdir().unwrap();
    let dirs = store(&root);
    dirs.ensure().unwrap();
    let _start = crate::crash::store::check_with(Some(&dirs.data), false, &FakeFs::new(), false);
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
    let (normal, _) = super::operations(
        (Some(&dirs), None),
        &home(),
        false,
        &(std::sync::Arc::new(navo_fs::LocalFs::new()) as std::sync::Arc<dyn navo_fs::ProviderOps>),
    );
    assert!(normal.retain_deletes.is_none());
    let (experimental, _) = super::operations(
        (Some(&dirs), None),
        &home(),
        true,
        &(std::sync::Arc::new(navo_fs::LocalFs::new()) as std::sync::Arc<dyn navo_fs::ProviderOps>),
    );
    let active = experimental.retain_deletes.unwrap();
    assert!(active().is_some());
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-deletes = false\n",
    )
    .unwrap();
    assert!(active().is_none());
}

mod main_compat;

mod startup;

use rusty_core::Location;
use rusty_fs::FakeFs;
use rusty_store::StoreDirs;

use super::operations;
use crate::jobs;

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
    let journal = jobs::open(&dirs.data.join("ferrite.db")).unwrap();
    journal
        .lock()
        .unwrap()
        .create_job("copy", b"not a plan", 0, 0)
        .unwrap();
}

fn running_jobs(dirs: &StoreDirs) -> usize {
    let journal = jobs::open(&dirs.data.join("ferrite.db")).unwrap();
    let guard = journal.lock().unwrap();
    guard.unfinished().unwrap().len()
}

#[test]
fn without_store_folders_only_home_is_protected() {
    let (ops, lock) = operations(None, &home(), &FakeFs::new());
    assert_eq!(ops.protected, [home()]);
    assert!(lock.is_none());
}

#[test]
fn store_folders_are_protected_and_get_a_journal() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let (ops, lock) = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(ops.protected.len(), 3);
    assert!(dirs.data.join("ferrite.db").exists());
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

#[test]
fn settings_read_is_marked_after_loading_finishes() {
    let mut completed = None;
    let _settings = super::finish_settings(
        || {
            completed = Some(std::time::Instant::now());
            None
        },
        &crate::search_hooks::Remember::default(),
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
    let quit = super::quitter(None, crate::profile::Cleanup::default(), Some(running));
    quit();
    assert!(!dir.path().join("running").exists());
    assert!(crate::crash::pending(dir.path()).is_none());
}

#[test]
fn a_failed_integrity_check_on_record_keeps_the_journal_closed_and_the_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let db = dirs.data.join("ferrite.db");
    crate::crash::test_damage::fill_history(&db);
    crate::crash::test_damage::corrupt_middle_page(&db);
    let bytes = std::fs::read(&db).unwrap();
    let sentinel = crate::crash::sentinel_path(&db);
    std::fs::write(&sentinel, b"").unwrap();
    let (ops, lock) = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(ops.protected.len(), 3);
    drop((ops, lock));
    assert!(sentinel.exists());
    assert_eq!(std::fs::read(&db).unwrap(), bytes);
    let (opened, _) = rusty_store::Db::open(&db, false).unwrap();
    let journal = rusty_store::JobJournal::new(opened).unwrap();
    assert_eq!(journal.unfinished().unwrap().len(), 1);
}

#[test]
fn a_corrupt_journal_found_during_recovery_is_put_on_record() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let db = dirs.data.join("ferrite.db");
    let page = crate::crash::test_damage::root_page(&db, "job");
    crate::crash::test_damage::corrupt_page(&db, page);
    let _started = operations(Some(&dirs), &home(), &FakeFs::new());
    assert!(crate::crash::sentinel_path(&db).exists());
}

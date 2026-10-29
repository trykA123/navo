use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use rusty_store::{Db, backup_path};
use rusty_ui::settings::{Change, Values};
use rusty_ui::tokens::ThemeChoice;

use super::{Loaded, follow_defaults, hooks, spawn, start, start_sharing, start_waiting};

struct Paths {
    _dir: tempfile::TempDir,
    file: PathBuf,
    ledger: PathBuf,
}

fn paths() -> Paths {
    let dir = tempfile::tempdir().unwrap();
    Paths {
        file: dir.path().join("navo.toml"),
        ledger: dir.path().join("navo.db"),
        _dir: dir,
    }
}

fn recorded(ledger: &Path) -> usize {
    let (db, _) = Db::open(ledger, false).unwrap();
    db.settings_defaults().unwrap().len()
}

#[test]
fn a_fresh_start_writes_every_setting_and_records_the_defaults() {
    let at = paths();
    let started = start(Some(&at.file), Some(&at.ledger));
    let text = fs::read_to_string(&at.file).unwrap();
    for key in [
        "mode = \"system\"",
        "sort = \"name\"",
        "provider = \"system\"",
        "[text.view]\nline-numbers = true",
        "[copy]\nmode = \"auto\"",
        "[automotive.view]\npreset = \"simple\"",
    ] {
        assert!(text.contains(key), "{key} missing from {text}");
    }
    assert_eq!(recorded(&at.ledger), 68);
    assert_eq!(started.default_changes, []);
    let settings = hooks(started.loaded);
    assert_eq!(settings.values, Values::default());
    assert!(settings.load_error.is_none());
}

#[test]
fn a_second_start_writes_nothing() {
    let at = paths();
    let _ = start(Some(&at.file), Some(&at.ledger));
    let first = fs::read(&at.file).unwrap();
    let started = start(Some(&at.file), Some(&at.ledger));
    assert_eq!(fs::read(&at.file).unwrap(), first);
    assert!(!backup_path(&at.file).exists());
    assert_eq!(started.default_changes, []);
}

#[test]
fn an_unparsable_file_is_reported_and_never_rewritten() {
    let at = paths();
    let broken = "[appearance]\nfont-size = \"big\"\n";
    fs::write(&at.file, broken).unwrap();
    let started = start(Some(&at.file), Some(&at.ledger));
    assert_eq!(fs::read(&at.file).unwrap(), broken.as_bytes());
    assert!(!backup_path(&at.file).exists());
    let settings = hooks(started.loaded);
    let reported = settings.load_error.unwrap();
    assert!(reported.contains("line 2"), "{reported}");
}

#[test]
fn without_a_ledger_the_file_is_left_as_it_is() {
    let at = paths();
    let mine = "[appearance]\nmode = \"dark\"\n";
    fs::write(&at.file, mine).unwrap();
    let started = start(Some(&at.file), None);
    assert_eq!(fs::read_to_string(&at.file).unwrap(), mine);
    assert!(started.loaded.is_some());
}

#[test]
fn without_a_settings_file_nothing_is_loaded() {
    let at = paths();
    let started = start(None, Some(&at.ledger));
    assert!(started.loaded.is_none());
    assert!(!at.file.exists());
}

#[test]
fn a_sheet_save_after_the_start_keeps_the_backup_from_before_the_session() {
    let at = paths();
    let old = "# old\n[appearance]\nmode = \"light\"\n";
    fs::write(&at.file, old).unwrap();
    let started = start(Some(&at.file), Some(&at.ledger));
    let settings = hooks(started.loaded);
    let report = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert!(report.error.is_none(), "{:?}", report.error);
    assert_eq!(fs::read_to_string(backup_path(&at.file)).unwrap(), old);
    let text = fs::read_to_string(&at.file).unwrap();
    assert!(
        text.starts_with("# old\n[appearance]\nmode = \"dark\"\n"),
        "{text}"
    );
}

#[test]
fn the_background_start_gives_the_same_result() {
    let at = paths();
    let started = start_waiting(
        Some(at.file.clone()),
        Some(at.ledger.clone()),
        Duration::from_secs(60),
    )
    .finish();
    assert!(started.loaded.is_some());
    assert_eq!(recorded(&at.ledger), 68);
    assert!(
        fs::read_to_string(&at.file)
            .unwrap()
            .contains("[selection]")
    );
}

#[test]
fn loading_alone_never_writes() {
    let at = paths();
    let loaded = Loaded::load(&at.file);
    assert!(loaded.report.error.is_none());
    assert!(!at.file.exists());
}

fn sorted_by_date() -> rusty_store::Settings {
    let mut settings = rusty_store::Settings::default();
    settings.files.sort = rusty_store::SortKey::Date;
    settings
}

fn set_ledger_writes(ledger: &Path, failing: bool) {
    let (db, _) = Db::open(ledger, false).unwrap();
    let sql = if failing {
        "CREATE TRIGGER no_insert BEFORE INSERT ON settings_defaults
           BEGIN SELECT RAISE(FAIL, 'ledger is full'); END;
         CREATE TRIGGER no_update BEFORE UPDATE ON settings_defaults
           BEGIN SELECT RAISE(FAIL, 'ledger is full'); END;"
    } else {
        "DROP TRIGGER no_insert; DROP TRIGGER no_update;"
    };
    db.connection().execute_batch(sql).unwrap();
}

fn recorded_sort(ledger: &Path) -> Option<String> {
    let (db, _) = Db::open(ledger, false).unwrap();
    db.settings_defaults().unwrap().get("files.sort").cloned()
}

#[test]
fn a_failed_ledger_write_keeps_the_change_notice_and_the_next_start_adopts_the_key() {
    let at = paths();
    let _ = start(Some(&at.file), Some(&at.ledger));
    set_ledger_writes(&at.ledger, true);
    let mut loaded = Loaded::load(&at.file);
    let changes = follow_defaults(&mut loaded.store, &at.ledger, &sorted_by_date());
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].key, "files.sort");
    assert_eq!(recorded_sort(&at.ledger).as_deref(), Some("\"name\""));
    set_ledger_writes(&at.ledger, false);
    let mut next = Loaded::load(&at.file);
    let again = follow_defaults(&mut next.store, &at.ledger, &sorted_by_date());
    assert_eq!(again, []);
    assert_eq!(recorded_sort(&at.ledger).as_deref(), Some("\"date\""));
}

#[test]
fn a_slow_completion_does_not_hold_up_the_start_or_lose_a_later_save() {
    let at = paths();
    let old = "# old\n[appearance]\nmode = \"light\"\n";
    fs::write(&at.file, old).unwrap();
    let read = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (read_worker, gate) = (read.clone(), release.clone());
    let (done, finished) = mpsc::channel::<()>();
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file.clone()), Duration::ZERO, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        read_worker.wait();
        gate.wait();
        started.default_changes = follow_defaults(
            &mut started.loaded.as_mut().unwrap().store,
            &ledger,
            &rusty_store::Settings::default(),
        );
        let _ = done.send(());
        started
    });
    read.wait();
    let started = starting.finish();
    assert_eq!(fs::read_to_string(&at.file).unwrap(), old);
    assert_eq!(started.default_changes, []);
    let settings = hooks(started.loaded);
    let report = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert!(report.error.is_none(), "{:?}", report.error);
    release.wait();
    finished.recv_timeout(Duration::from_secs(10)).unwrap();
    let text = fs::read_to_string(&at.file).unwrap();
    assert!(
        text.starts_with("# old\n[appearance]\nmode = \"dark\"\n"),
        "{text}"
    );
    assert!(text.contains("[selection]"), "{text}");
    assert_eq!(fs::read_to_string(backup_path(&at.file)).unwrap(), old);
}

#[test]
fn a_start_that_fails_loads_the_file_as_it_is() {
    let at = paths();
    let mine = "[appearance]\nmode = \"dark\"\n";
    fs::write(&at.file, mine).unwrap();
    let starting = spawn(Some(at.file.clone()), Duration::from_secs(10), |_| {
        panic!("the completion thread died")
    });
    let started = starting.finish();
    let settings = hooks(started.loaded);
    assert_eq!(settings.values.theme(), ThemeChoice::Dark);
    assert_eq!(fs::read_to_string(&at.file).unwrap(), mine);
}

#[test]
fn late_completion_before_watcher_registration_reaches_the_live_store() {
    use rusty_ui::window_view::SettingsNews;
    let at = paths();
    let _ = start(Some(&at.file), Some(&at.ledger));
    let read = Arc::new(Barrier::new(2));
    let replace = Arc::new(Barrier::new(2));
    let committed = Arc::new(Barrier::new(2));
    let (read_worker, replace_worker, committed_worker) =
        (read.clone(), replace.clone(), committed.clone());
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file), Duration::ZERO, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        read_worker.wait();
        replace_worker.wait();
        let loaded = started.loaded.as_mut().unwrap();
        let mut defaults = rusty_store::Settings::default();
        defaults.appearance.font_size = 18;
        started.default_changes = follow_defaults(&mut loaded.store, &ledger, &defaults);
        committed_worker.wait();
        started
    });
    read.wait();
    let started = starting.finish();
    let loaded = started.loaded.unwrap();
    assert_eq!(loaded.store.current().appearance.font_size, 13);
    replace.wait();
    committed.wait();
    let (send, news) = mpsc::channel();
    let (settings, _watch) = super::watched(Some(loaded), move |event| {
        let _ = send.send(event);
    });
    let SettingsNews::Applied { values, generation } =
        news.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!("completion applies");
    };
    assert_eq!(
        values.get(rusty_ui::settings::Field::FontSize),
        &rusty_ui::settings::Value::Number(18)
    );
    let saved = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert!(saved.generation > generation);
    assert_eq!(
        saved.values.get(rusty_ui::settings::Field::FontSize),
        &rusty_ui::settings::Value::Number(18)
    );
}

#[test]
fn late_completion_reaches_the_live_store_without_a_watch_event() {
    use std::sync::Mutex;
    let at = paths();
    let _ = start(Some(&at.file), Some(&at.ledger));
    let read = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (read_worker, gate) = (read.clone(), release.clone());
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file), Duration::ZERO, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        read_worker.wait();
        gate.wait();
        let mut defaults = rusty_store::Settings::default();
        defaults.appearance.font_size = 18;
        started.default_changes = follow_defaults(
            &mut started.loaded.as_mut().unwrap().store,
            &ledger,
            &defaults,
        );
        started
    });
    read.wait();
    let mut loaded = starting.finish().loaded.unwrap();
    let receiver = loaded.late.take();
    let store = Arc::new(Mutex::new(loaded.store));
    let (send, news) = mpsc::channel();
    let delivery =
        super::reload::Delivery::new(super::Reloader::new(store.clone(), None), move |event| {
            let _ = send.send(event);
        });
    super::reload::receive_late(receiver, delivery);
    release.wait();
    let event = news.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(
        event,
        rusty_ui::window_view::SettingsNews::Applied { .. }
    ));
    assert_eq!(store.lock().unwrap().current().appearance.font_size, 18);
}

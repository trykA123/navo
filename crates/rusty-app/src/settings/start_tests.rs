use std::fs;
use std::path::{Path, PathBuf};

use rusty_store::{Db, backup_path};
use rusty_ui::settings::{Change, Values};
use rusty_ui::tokens::ThemeChoice;

use super::{Loaded, hooks, start, start_in_background};

struct Paths {
    _dir: tempfile::TempDir,
    file: PathBuf,
    ledger: PathBuf,
}

fn paths() -> Paths {
    let dir = tempfile::tempdir().unwrap();
    Paths {
        file: dir.path().join("ferrite.toml"),
        ledger: dir.path().join("ferrite.db"),
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
        "provider = \"windows-terminal\"",
    ] {
        assert!(text.contains(key), "{key} missing from {text}");
    }
    assert_eq!(recorded(&at.ledger), 14);
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
    let started = start_in_background(Some(at.file.clone()), Some(at.ledger.clone())).finish();
    assert!(started.loaded.is_some());
    assert_eq!(recorded(&at.ledger), 14);
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

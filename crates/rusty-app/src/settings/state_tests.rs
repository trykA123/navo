use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusty_store::Db;
use rusty_ui::settings::{Change, Field};
use rusty_ui::tokens::Density;
use rusty_ui::window_view::{SettingsHooks, SettingsNews};

use super::{Reloader, hooks, start};

struct Paths {
    _dir: tempfile::TempDir,
    file: PathBuf,
    ledger: PathBuf,
}

fn started() -> (Paths, SettingsHooks) {
    let dir = tempfile::tempdir().unwrap();
    let at = Paths {
        file: dir.path().join("ferrite.toml"),
        ledger: dir.path().join("ferrite.db"),
        _dir: dir,
    };
    let loaded = start(Some(&at.file), Some(&at.ledger)).loaded;
    (at, hooks(loaded))
}

fn recorded_density(at: &Paths) -> Option<String> {
    let (db, _) = Db::open(&at.ledger, false).unwrap();
    db.settings_defaults()
        .unwrap()
        .get("appearance.density")
        .cloned()
}

#[test]
fn a_completed_file_starts_with_no_changed_row() {
    let (_at, settings) = started();
    assert_eq!(settings.values.changed_count(), 0);
}

#[test]
fn a_sheet_save_marks_the_row_changed() {
    let (_at, settings) = started();
    let report = (settings.save)(&[Change::density(Density::Compact)]);
    assert!(report.values.is_changed(Field::Density));
    assert_eq!(report.values.changed_count(), 1);
}

#[test]
fn reset_writes_the_default_and_the_ledger_follows_it() {
    let (at, settings) = started();
    (settings.save)(&[Change::density(Density::Compact)]);
    let report = (settings.save)(&[Change::reset(Field::Density)]);
    assert_eq!(report.error, None);
    assert!(!report.values.is_changed(Field::Density));
    assert_eq!(report.values.density(), Density::Normal);
    let text = fs::read_to_string(&at.file).unwrap();
    assert!(text.contains("density = \"normal\""), "{text}");
    assert_eq!(recorded_density(&at).as_deref(), Some("\"normal\""));
}

#[test]
fn a_hand_edit_marks_the_row_changed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let ledger = dir.path().join("ferrite.db");
    let loaded = start(Some(&file), Some(&ledger)).loaded.unwrap();
    let mut reloader = Reloader::new(Arc::new(Mutex::new(loaded.store)), None);
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        text.replace("density = \"normal\"", "density = \"compact\""),
    )
    .unwrap();
    let Some(SettingsNews::Applied { values, .. }) = reloader.reload() else {
        panic!("the hand edit applies");
    };
    assert!(values.is_changed(Field::Density));
    assert_eq!(values.changed_count(), 1);
}

#[test]
fn reset_with_an_unavailable_ledger_writes_the_default_and_reports_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let ledger = dir.path().join("ferrite.db");
    let mut loaded = start(Some(&file), Some(&ledger)).loaded.unwrap();
    loaded.ledger = Some(dir.path().to_owned());
    let settings = hooks(Some(loaded));
    (settings.save)(&[Change::density(Density::Compact)]);
    let report = (settings.save)(&[Change::reset(Field::Density)]);
    assert_eq!(report.failed, []);
    assert!(!report.values.is_changed(Field::Density));
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("density = \"normal\""), "{text}");
    let message = report.error.unwrap_or_default();
    assert!(message.contains("ledger"), "{message}");
}

#[test]
fn reset_of_a_key_without_a_default_removes_it_and_saving_the_old_value_brings_it_back() {
    let (at, settings) = started();
    (settings.save)(&[Change::text(Field::EditorPath, "code")]);
    let report = (settings.save)(&[Change::reset(Field::EditorPath)]);
    assert!(!report.values.is_changed(Field::EditorPath));
    assert!(!fs::read_to_string(&at.file).unwrap().contains("\"code\""));
    let report = (settings.save)(&[Change::text(Field::EditorPath, "code")]);
    assert!(report.values.is_changed(Field::EditorPath));
    assert!(
        fs::read_to_string(&at.file)
            .unwrap()
            .contains("path = \"code\"")
    );
}

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusty_store::{Settings, SettingsStore, ThemeMode};
use rusty_ui::settings::{Change, Control, FIELDS, Field, Value, Values};
use rusty_ui::tokens::{Density, ThemeChoice};
use rusty_ui::window_view::{KeyLine, SettingsHooks, SettingsNews};

use super::{Loaded, Reloader, values_of};

fn hooks(path: Option<&Path>) -> SettingsHooks {
    super::hooks(path.map(Loaded::load))
}

const OLD_EDITOR_FILE: &str = "[files]\nshow-hidden = false\n\n[editor]\npath = \"code\"\n";

fn no_ledger() -> Mutex<super::LedgerFile> {
    Mutex::new(super::LedgerFile::new(None))
}

fn file_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("ferrite.toml")
}

fn other_value(spec: &rusty_ui::settings::FieldSpec, now: &Value) -> Value {
    match (spec.control, now) {
        (Control::Choices(options, _), Value::Pick(at)) => Value::Pick((at + 1) % options.len()),
        (_, Value::Flag(on)) => Value::Flag(!on),
        (_, Value::Number(number)) => Value::Number(number + 2),
        _ => Value::Text("something else".into()),
    }
}

fn save(settings: &SettingsHooks, change: Change) -> Result<Values, String> {
    let report = (settings.save)(&[change]);
    match report.error {
        Some(message) => Err(message),
        None => Ok(report.values),
    }
}

#[test]
fn the_defaults_match_on_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = SettingsStore::load(&file_in(&dir));
    assert_eq!(store.current(), &Settings::default());
    assert_eq!(values_of(&store), Values::default());
}

#[test]
fn every_field_reaches_the_file_and_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    for spec in &FIELDS {
        let settings = hooks(Some(&file));
        let value = other_value(spec, settings.values.get(spec.field));
        let change = Change {
            field: spec.field,
            value: value.clone(),
        };
        let stored = save(&settings, change).unwrap();
        assert_eq!(stored.get(spec.field), &value, "{}", spec.key);
        let again = hooks(Some(&file));
        assert_eq!(again.values.get(spec.field), &value, "{}", spec.key);
        assert!(again.load_error.is_none(), "{}", spec.key);
    }
}

#[test]
fn a_sheet_save_after_deletion_restores_all_current_settings() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    let changes: Vec<_> = FIELDS
        .iter()
        .map(|spec| Change {
            field: spec.field,
            value: other_value(spec, settings.values.get(spec.field)),
        })
        .collect();
    let report = (settings.save)(&changes);
    assert_eq!(report.error, None);
    let change = Change::theme(ThemeChoice::Dark);
    let expected = report.values.with(&change);
    fs::remove_file(&file).unwrap();
    let stored = save(&settings, change).unwrap();
    let again = hooks(Some(&file));
    assert!(again.load_error.is_none());
    assert_eq!(again.values, expected);
    assert_eq!(stored, expected);
    for spec in &FIELDS {
        let found = (again.line_of)(spec.key);
        assert!(found.is_some_and(|found| !found.is_missing), "{}", spec.key);
    }
}

#[test]
fn the_menu_choices_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    let changes = [
        Change::theme(ThemeChoice::Dark),
        Change::density(Density::Compact),
        Change::show_hidden(false),
    ];
    let stored = (settings.save)(&changes).values;
    assert_eq!(stored.theme(), ThemeChoice::Dark);
    let (store, _) = SettingsStore::load(&file);
    assert_eq!(store.current().appearance.mode, ThemeMode::Dark);
    let again = hooks(Some(&file)).values;
    assert_eq!(again.density(), Density::Compact);
    assert!(!again.show_hidden());
}

#[test]
fn a_save_writes_only_its_key_and_keeps_comments_and_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, "# mine\n[appearance]\nzoom = 2\n").unwrap();
    let settings = hooks(Some(&file));
    save(&settings, Change::theme(ThemeChoice::Dark)).unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# mine\n[appearance]\nzoom = 2\nmode = \"dark\"\n"
    );
}

#[test]
fn a_save_keeps_hand_edits_made_after_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    fs::write(&file, "[appearance]\ndensity = \"comfortable\"\n").unwrap();
    let stored = save(&settings, Change::theme(ThemeChoice::Light)).unwrap();
    assert_eq!(stored.theme(), ThemeChoice::Light);
    assert_eq!(stored.density(), Density::Comfortable);
}

#[test]
fn undo_writes_the_old_value_back_and_the_file_returns() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let before = "# mine\n[appearance]\nmode = \"light\"\n";
    fs::write(&file, before).unwrap();
    let settings = hooks(Some(&file));
    let old = settings.values.change_of(Field::Theme);
    save(&settings, Change::theme(ThemeChoice::Dark)).unwrap();
    let stored = save(&settings, old).unwrap();
    assert_eq!(stored.theme(), ThemeChoice::Light);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn an_empty_text_removes_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, "[editor]\npath = \"code\"\n").unwrap();
    let settings = hooks(Some(&file));
    assert_eq!(settings.values.editor_path().as_deref(), Some("code"));
    let stored = save(&settings, Change::text(Field::EditorPath, "")).unwrap();
    assert_eq!(stored.editor_path(), None);
    assert_eq!(fs::read_to_string(&file).unwrap(), "[editor]\n");
}

#[test]
fn an_old_editor_written_file_keeps_hidden_files_shown_and_stays_valid() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, OLD_EDITOR_FILE).unwrap();
    let settings = hooks(Some(&file));
    assert!(settings.values.show_hidden());
    assert_eq!(settings.values.editor_path().as_deref(), Some("code"));
    assert!(settings.load_error.is_none());
}

#[test]
fn an_invalid_file_is_reported_and_left_byte_identical_by_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let broken = "[appearance]\nfont-size = \"big\"\n";
    fs::write(&file, broken).unwrap();
    let settings = hooks(Some(&file));
    let reported = settings.load_error.clone().unwrap();
    assert!(reported.contains("ferrite.toml"), "{reported}");
    assert!(reported.contains("line 2"), "{reported}");
    let failed = save(&settings, Change::theme(ThemeChoice::Dark)).unwrap_err();
    assert!(failed.contains("ferrite.toml"), "{failed}");
    assert_eq!(fs::read_to_string(&file).unwrap(), broken);
}

#[test]
fn without_a_settings_file_changes_stay_for_the_session() {
    let settings = hooks(None);
    assert!(settings.values.show_hidden());
    save(&settings, Change::theme(ThemeChoice::Dark)).unwrap();
    let stored = save(&settings, Change::density(Density::Compact)).unwrap();
    assert_eq!(stored.theme(), ThemeChoice::Dark);
    assert_eq!(stored.density(), Density::Compact);
    assert!(settings.file.is_none());
    assert_eq!((settings.line_of)("appearance.mode"), None);
}

#[test]
fn the_line_of_a_key_is_read_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, "# mine\n[appearance]\ndensity = \"compact\"\n").unwrap();
    let settings = hooks(Some(&file));
    let density = KeyLine {
        line: 3,
        is_missing: false,
    };
    assert_eq!((settings.line_of)("appearance.density"), Some(density));
    let mode = KeyLine {
        line: 2,
        is_missing: true,
    };
    assert_eq!((settings.line_of)("appearance.mode"), Some(mode));
    let location = settings.file.expect("a settings file");
    let name = Path::new(location.as_str()).file_name();
    assert_eq!(name.and_then(|name| name.to_str()), Some("ferrite.toml"));
}

#[test]
fn a_failed_change_does_not_stop_the_rest_of_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    let broken = Change::text(Field::FontSize, "big");
    let report = (settings.save)(&[broken, Change::theme(ThemeChoice::Dark)]);
    assert_eq!(report.failed, [Field::FontSize]);
    assert!(
        report
            .error
            .is_some_and(|message| message.contains("ferrite.toml"))
    );
    assert_eq!(report.values.theme(), ThemeChoice::Dark);
    assert_eq!(report.values.get(Field::FontSize), &Value::Number(13));
    assert_eq!(hooks(Some(&file)).values.theme(), ThemeChoice::Dark);
}

#[test]
fn undo_of_a_key_that_was_not_in_the_file_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    let report = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert_eq!(report.was_absent, [Field::Theme]);
    let undo = Change {
        field: Field::Theme,
        value: Value::Unset,
    };
    let report = (settings.save)(&[undo]);
    assert_eq!(report.values.theme(), ThemeChoice::System);
    let text = fs::read_to_string(&file).unwrap();
    assert!(!text.contains("mode"), "{text}");
}

#[test]
fn saves_and_reloads_carry_a_rising_generation() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, "[appearance]\nmode = \"light\"\n").unwrap();
    let (store, _) = SettingsStore::load(&file);
    let store = Arc::new(Mutex::new(store));
    let mut reloader = Reloader::new(store.clone(), None);
    let first = super::save(&store, &no_ledger(), &[Change::theme(ThemeChoice::Dark)]);
    fs::write(&file, "[appearance]\nmode = \"light\"\n").unwrap();
    let Some(SettingsNews::Applied { generation, .. }) = reloader.reload() else {
        panic!("the hand edit applies");
    };
    assert!(generation > first.generation);
    let second = super::save(&store, &no_ledger(), &[Change::density(Density::Compact)]);
    assert!(second.generation > generation);
}

#[test]
fn a_sheet_save_is_refused_while_the_file_on_disk_is_broken() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(&file, "[appearance]\ndensity = \"compact\"\n").unwrap();
    let settings = hooks(Some(&file));
    let broken = "[appearance]\ndensity = \n";
    fs::write(&file, broken).unwrap();
    let report = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert_eq!(report.failed, [Field::Theme]);
    let message = report.error.unwrap();
    assert!(message.contains("invalid"), "{message}");
    assert_eq!(report.values.density(), Density::Compact);
    assert_eq!(report.values.theme(), ThemeChoice::System);
    assert_eq!(fs::read_to_string(&file).unwrap(), broken);
    assert!(!rusty_store::backup_path(&file).exists());
    fs::write(&file, "[appearance]\ndensity = \"compact\"\n").unwrap();
    let fixed = (settings.save)(&[Change::theme(ThemeChoice::Dark)]);
    assert_eq!(fixed.error, None);
    assert_eq!(fixed.values.theme(), ThemeChoice::Dark);
}

#[test]
fn a_kept_view_part_lands_in_its_viewer_table_and_keeps_comments() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    fs::write(
        &file,
        "# mine\n[text.view]\nline-numbers = true # gutter\nline-ends = false\n",
    )
    .unwrap();
    let settings = hooks(Some(&file));
    let kept = Change {
        field: Field::TextLineEnds,
        value: Value::Flag(true),
    };
    let stored = save(&settings, kept).unwrap();
    assert_eq!(stored.get(Field::TextLineEnds), &Value::Flag(true));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# mine\n[text.view]\nline-numbers = true # gutter\nline-ends = true\n"
    );
}

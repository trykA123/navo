use std::fs;
use std::path::Path;

use rusty_store::{Settings, SettingsStore, ThemeMode};
use rusty_ui::settings::{Change, Control, FIELDS, Field, Value, Values};
use rusty_ui::tokens::{Density, ThemeChoice};
use rusty_ui::window_view::SettingsHooks;

use super::{hooks, values_of};

const OLD_EDITOR_FILE: &str = "[files]\nshow-hidden = false\n\n[editor]\npath = \"code\"\n";

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
    (settings.save)(&[change])
}

#[test]
fn the_defaults_match_on_both_sides() {
    assert_eq!(values_of(&Settings::default()), Values::default());
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
fn the_menu_choices_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = file_in(&dir);
    let settings = hooks(Some(&file));
    let changes = [
        Change::theme(ThemeChoice::Dark),
        Change::density(Density::Compact),
        Change::show_hidden(false),
    ];
    let stored = (settings.save)(&changes).unwrap();
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
    assert_eq!((settings.line_of)("appearance.density"), Some(3));
    assert_eq!((settings.line_of)("appearance.mode"), None);
    let location = settings.file.expect("a settings file");
    let name = Path::new(location.as_str()).file_name();
    assert_eq!(name.and_then(|name| name.to_str()), Some("ferrite.toml"));
}

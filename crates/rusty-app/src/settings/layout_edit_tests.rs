use std::fs;
use std::path::{Path, PathBuf};

use rusty_core::Tiles;
use rusty_ui::window_view::{LayoutEdit, SaveReport, SettingsHooks};

use super::{Loaded, hooks};

const ORIGINAL: &str = "# my settings\n[files]\nshow-hidden = true # keep me\n";
const COLUMNS: &str = "row(50, pane, pane)";
const ROWS: &str = "col(40, pane, pane)";

fn setup(text: &str) -> (tempfile::TempDir, PathBuf, SettingsHooks) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.toml");
    fs::write(&path, text).unwrap();
    let settings = hooks(Some(Loaded::load(&path)));
    (dir, path, settings)
}

fn save(name: &str, tiles: &str) -> LayoutEdit {
    LayoutEdit::Save {
        name: name.to_owned(),
        tiles: tiles.to_owned(),
    }
}

fn names(report: &SaveReport) -> Vec<String> {
    let presets = report.values.layout_presets();
    presets.iter().map(|(name, _)| name.to_string()).collect()
}

fn set_read_only(path: &Path, is_read_only: bool) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(is_read_only);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn saving_a_preset_adds_a_table_and_keeps_an_unrelated_comment() {
    let (_dir, path, settings) = setup(ORIGINAL);
    let report = (settings.edit_layout)(&save("Review", COLUMNS)).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(ORIGINAL), "{text}");
    assert!(
        text.contains("[[layout.presets]]\nname = \"Review\"\ntiles = \"row(50, pane, pane)\"")
    );
    assert_eq!(names(&report), ["Review"]);
    assert!(report.values.show_hidden());
    assert!(report.generation > 0);
}

#[test]
fn replacing_keeps_the_place_and_removing_drops_the_table() {
    let (_dir, path, settings) = setup(ORIGINAL);
    (settings.edit_layout)(&save("Review", COLUMNS)).unwrap();
    (settings.edit_layout)(&save("Logs", COLUMNS)).unwrap();
    let replace = LayoutEdit::Replace {
        name: "Review".to_owned(),
        tiles: ROWS.to_owned(),
    };
    let report = (settings.edit_layout)(&replace).unwrap();
    assert_eq!(names(&report), ["Review", "Logs"]);
    let rows = Tiles::parse(ROWS).unwrap();
    assert_eq!(report.values.layout_presets()[0].1, rows);
    for name in ["Review", "Logs"] {
        let remove = LayoutEdit::Remove {
            name: name.to_owned(),
        };
        (settings.edit_layout)(&remove).unwrap();
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
}

#[test]
fn a_write_error_leaves_the_file_unchanged() {
    let (_dir, path, settings) = setup(ORIGINAL);
    set_read_only(&path, true);
    let refused = (settings.edit_layout)(&save("Review", COLUMNS));
    set_read_only(&path, false);
    let error = refused.unwrap_err();
    assert!(error.contains("navo.toml"), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
}

#[test]
fn the_twenty_first_preset_is_refused_and_the_file_unchanged() {
    let full: String = (1..=20)
        .map(|n| format!("[[layout.presets]]\nname = \"P{n}\"\ntiles = \"pane\"\n"))
        .collect();
    let (_dir, path, settings) = setup(&full);
    assert!((settings.edit_layout)(&save("Extra", COLUMNS)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), full);
}

#[test]
fn without_a_settings_file_preset_edits_report_an_error() {
    assert!((hooks(None).edit_layout)(&save("Review", COLUMNS)).is_err());
}

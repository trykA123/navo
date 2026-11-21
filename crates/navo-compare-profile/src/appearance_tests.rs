//! Appearance tests: `[ui]` round trips, foreign tables and `.bak` recovery.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::appearance::{
    AppearanceMode, LayoutPref, PaletteId, UiPrefs, UpdateCheck, ViewModePref, ViewPresetPref,
    load_or_default, save,
};
use crate::brand;
use crate::paths::{Folders, Profile};

/// A unique temporary folder, removed when the test ends.
pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty folder named after the test and a counter.
    pub(super) fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let folder = format!(
            "{}-appearance-{}-{name}-{count}",
            brand::SLUG,
            std::process::id()
        );
        let path = std::env::temp_dir().join(folder);
        fs::create_dir_all(&path).expect("temp folder");
        TempDir { path }
    }

    /// Returns the path of `name` inside the folder.
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    /// Removes the folder and everything in it, ignoring errors.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Opens a profile whose config and data folders are under `temp`.
pub(super) fn open_profile(temp: &TempDir) -> Profile {
    Profile::open(Folders {
        config: temp.join("config"),
        data: temp.join("data"),
    })
    .expect("open profile")
}

/// Writes `text` as the settings file of `profile`.
pub(super) fn write_settings(profile: &Profile, text: &str) {
    fs::write(profile.settings_file(), text).expect("write settings");
}

/// Returns the text of the settings file of `profile`.
pub(super) fn settings_text(profile: &Profile) -> String {
    fs::read_to_string(profile.settings_file()).expect("read settings")
}

/// Returns the bytes of `config/settings.toml.bak`.
fn backup_bytes(profile: &Profile) -> Vec<u8> {
    fs::read(profile.settings_file().with_extension("toml.bak")).expect("read backup")
}

#[test]
fn a_saved_palette_is_read_back() {
    let temp = TempDir::new("palette");
    let profile = open_profile(&temp);
    let prefs = UiPrefs {
        palette: PaletteId::Ember,
        ..UiPrefs::default()
    };
    save(&profile, &prefs).expect("save");
    assert_eq!(
        load_or_default(&profile).expect("load").palette,
        PaletteId::Ember,
        "the palette written to [ui] must come back"
    );
}

#[test]
fn every_palette_round_trips() {
    let temp = TempDir::new("palettes");
    let profile = open_profile(&temp);
    for palette in [
        PaletteId::Violet,
        PaletteId::Graphite,
        PaletteId::Ocean,
        PaletteId::Ember,
        PaletteId::Contrast,
    ] {
        let prefs = UiPrefs {
            palette,
            ..UiPrefs::default()
        };
        save(&profile, &prefs).expect("save");
        assert_eq!(
            load_or_default(&profile).expect("load").palette,
            palette,
            "every palette must survive a save and a load"
        );
    }
}

#[test]
fn every_appearance_mode_round_trips() {
    let temp = TempDir::new("modes");
    let profile = open_profile(&temp);
    for mode in [
        AppearanceMode::System,
        AppearanceMode::Light,
        AppearanceMode::Dark,
    ] {
        let prefs = UiPrefs {
            mode,
            ..UiPrefs::default()
        };
        save(&profile, &prefs).expect("save");
        assert_eq!(
            load_or_default(&profile).expect("load").mode,
            mode,
            "every mode must survive a save and a load"
        );
    }
}

#[test]
fn every_view_preset_and_view_mode_round_trips() {
    let temp = TempDir::new("presets");
    let profile = open_profile(&temp);
    for preset in [
        ViewPresetPref::Simple,
        ViewPresetPref::Rich,
        ViewPresetPref::Custom,
    ] {
        for view_mode in [ViewModePref::Best, ViewModePref::Text, ViewModePref::Bytes] {
            let prefs = UiPrefs {
                preset,
                view_mode,
                ..UiPrefs::default()
            };
            save(&profile, &prefs).expect("save");
            let loaded = load_or_default(&profile).expect("load");
            assert_eq!(
                (loaded.preset, loaded.view_mode),
                (preset, view_mode),
                "the preset and the kind of view must survive together"
            );
        }
    }
}

#[test]
fn the_view_toggles_round_trip() {
    let temp = TempDir::new("toggles");
    let profile = open_profile(&temp);
    let prefs = UiPrefs {
        palette: PaletteId::Contrast,
        mode: AppearanceMode::Dark,
        sidebar_collapsed: true,
        ui_scale_pct: 125,
        preset: ViewPresetPref::Custom,
        view_mode: ViewModePref::Bytes,
        layout: LayoutPref::Unified,
        words: false,
        syntax: false,
        line_no: false,
        fold_unchanged: false,
        wrap_lines: true,
        ignore_whitespace: true,
        update_check: UpdateCheck::On,
    };
    save(&profile, &prefs).expect("save");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        prefs,
        "every preference must come back, not only the palette"
    );
}

#[test]
fn the_document_names_the_ui_table_and_its_keys() {
    let temp = TempDir::new("names");
    let profile = open_profile(&temp);
    let prefs = UiPrefs {
        palette: PaletteId::Contrast,
        mode: AppearanceMode::Dark,
        sidebar_collapsed: true,
        preset: ViewPresetPref::Simple,
        view_mode: ViewModePref::Bytes,
        ..UiPrefs::default()
    };
    save(&profile, &prefs).expect("save");
    let text = settings_text(&profile);
    assert!(
        text.contains("[ui]\n"),
        "preferences need their own table: {text}"
    );
    assert!(
        text.contains("palette = \"contrast\""),
        "lowercase palette: {text}"
    );
    assert!(text.contains("mode = \"dark\""), "lowercase mode: {text}");
    assert!(
        text.contains("preset = \"simple\""),
        "lowercase preset: {text}"
    );
    assert!(
        text.contains("view_mode = \"bytes\""),
        "lowercase view mode: {text}"
    );
    assert!(
        text.contains("sidebar_collapsed = true"),
        "collapsed rail: {text}"
    );
}

#[test]
fn a_missing_settings_file_gives_the_defaults_and_is_not_created() {
    let temp = TempDir::new("missing");
    let profile = open_profile(&temp);
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs::default(),
        "a fresh profile starts on the defaults"
    );
    assert!(
        !profile.settings_file().exists(),
        "reading preferences must not write a settings file"
    );
}

#[test]
fn a_partial_ui_table_fills_the_rest_with_defaults() {
    let temp = TempDir::new("partial");
    let profile = open_profile(&temp);
    write_settings(&profile, "[ui]\npalette = \"ocean\"\nwrap_lines = true\n");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs {
            palette: PaletteId::Ocean,
            wrap_lines: true,
            ..UiPrefs::default()
        },
        "keys the file leaves out keep their defaults"
    );
}

#[test]
fn a_save_keeps_foreign_tables_and_top_level_keys() {
    let temp = TempDir::new("foreign");
    let profile = open_profile(&temp);
    write_settings(
        &profile,
        "version = 1\n\n[compare]\ngit-engine = \"git\"\nmemory-budget-mib = 1024\n\n[ui]\npalette = \"ember\"\n",
    );
    let prefs = UiPrefs {
        palette: PaletteId::Ocean,
        ..UiPrefs::default()
    };
    save(&profile, &prefs).expect("save");
    let text = settings_text(&profile);
    let document: toml::Table = toml::from_str(&text).expect("the saved document still parses");
    assert_eq!(
        document.get("version").and_then(toml::Value::as_integer),
        Some(1),
        "a top-level key must survive: {text}"
    );
    assert_eq!(
        document
            .get("compare")
            .and_then(|compare| compare.get("git-engine"))
            .and_then(toml::Value::as_str),
        Some("git"),
        "a table this build does not know must survive: {text}"
    );
    assert_eq!(
        document
            .get("compare")
            .and_then(|compare| compare.get("memory-budget-mib"))
            .and_then(toml::Value::as_integer),
        Some(1024),
        "every key of a foreign table must survive: {text}"
    );
    assert_eq!(
        load_or_default(&profile).expect("load"),
        prefs,
        "the new preferences replace the old ones"
    );
}

#[test]
fn broken_toml_moves_to_bak_and_gives_the_defaults() {
    let temp = TempDir::new("broken");
    let profile = open_profile(&temp);
    write_settings(&profile, "this is not toml [");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs::default(),
        "broken settings fall back to the defaults"
    );
    assert!(
        !profile.settings_file().exists(),
        "the broken file is moved aside, not left to be read again"
    );
    assert_eq!(
        backup_bytes(&profile),
        b"this is not toml [",
        "the broken bytes stay readable in the backup"
    );
}

#[test]
fn an_unknown_ui_value_moves_to_bak_and_gives_the_defaults() {
    let temp = TempDir::new("unknown-value");
    let profile = open_profile(&temp);
    write_settings(&profile, "[ui]\npalette = \"sunset\"\n");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs::default(),
        "a value this build does not know falls back to the defaults"
    );
    assert_eq!(
        backup_bytes(&profile),
        b"[ui]\npalette = \"sunset\"\n",
        "the unreadable preferences stay in the backup"
    );
}

#[test]
fn non_utf8_settings_move_to_bak_and_give_the_defaults() {
    let temp = TempDir::new("non-utf8");
    let profile = open_profile(&temp);
    fs::write(profile.settings_file(), [0x80, 0x81]).expect("write settings");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs::default(),
        "bytes that are not text fall back to the defaults"
    );
    assert_eq!(
        backup_bytes(&profile),
        vec![0x80, 0x81],
        "the unreadable bytes stay in the backup"
    );
}

#[test]
fn a_save_over_broken_toml_keeps_the_old_bytes_in_bak() {
    let temp = TempDir::new("save-over-broken");
    let profile = open_profile(&temp);
    write_settings(&profile, "broken [");
    let prefs = UiPrefs {
        palette: PaletteId::Ember,
        ..UiPrefs::default()
    };
    save(&profile, &prefs).expect("save");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        prefs,
        "a save replaces the broken document"
    );
    assert_eq!(
        backup_bytes(&profile),
        b"broken [",
        "the bytes that could not be read are kept, not overwritten"
    );
}

#[test]
fn a_save_leaves_no_temporary_file_behind() {
    let temp = TempDir::new("temp-clean");
    let profile = open_profile(&temp);
    save(&profile, &UiPrefs::default()).expect("save");
    let mut names: Vec<String> = fs::read_dir(temp.join("config"))
        .expect("read config folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["settings.toml".to_string()],
        "only the settings file is left in the config folder"
    );
}

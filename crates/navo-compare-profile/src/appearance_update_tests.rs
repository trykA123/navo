//! Update-check tests: the `[ui]` table stores whether Sidewise may check
//! GitHub for a new version (NC180).

use std::fs;

use crate::appearance::{
    AppearanceMode, LayoutPref, PaletteId, UiPrefs, UpdateCheck, ViewModePref, ViewPresetPref,
    load_or_default, save,
};
use crate::appearance_tests::{TempDir, open_profile, settings_text, write_settings};

/// A fresh profile asks before checking for updates.
#[test]
fn a_fresh_profile_asks_before_checking_for_updates() {
    let temp = TempDir::new("update-fresh");
    let profile = open_profile(&temp);
    assert_eq!(
        load_or_default(&profile).expect("load").update_check,
        UpdateCheck::Ask,
        "a fresh profile must ask before checking for updates"
    );
}

/// The on and off choices survive a save and a load, naming their key.
#[test]
fn update_check_choices_round_trip() {
    let temp = TempDir::new("update-round-trip");
    let profile = open_profile(&temp);
    for (choice, key) in [
        (UpdateCheck::On, "update_check = \"on\""),
        (UpdateCheck::Off, "update_check = \"off\""),
    ] {
        let prefs = UiPrefs {
            update_check: choice,
            ..UiPrefs::default()
        };
        save(&profile, &prefs).expect("save");
        assert_eq!(
            load_or_default(&profile).expect("load"),
            prefs,
            "the update check must survive a save and a load"
        );
        let text = settings_text(&profile);
        assert!(
            text.contains(key),
            "the saved text must name the update check: {text}"
        );
    }
}

/// An alpha.2 table without the key asks, and keeps its other values.
#[test]
fn an_alpha_two_table_without_update_check_keeps_its_values() {
    let temp = TempDir::new("update-alpha-two");
    let profile = open_profile(&temp);
    write_settings(
        &profile,
        "[ui]\npalette = \"ocean\"\nmode = \"dark\"\nsidebar_collapsed = true\nui_scale_pct = 125\npreset = \"custom\"\nview_mode = \"bytes\"\nlayout = \"unified\"\nwords = false\nsyntax = false\nline_no = false\nfold_unchanged = false\nwrap_lines = true\nignore_whitespace = true\n",
    );
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs {
            palette: PaletteId::Ocean,
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
            update_check: UpdateCheck::Ask,
        },
        "a table from alpha.2 must ask and keep every other value"
    );
}

/// An unknown update check value moves the file to `.bak` and defaults.
#[test]
fn an_unknown_update_check_moves_to_bak_and_gives_the_defaults() {
    let temp = TempDir::new("update-unknown");
    let profile = open_profile(&temp);
    write_settings(&profile, "[ui]\nupdate_check = \"maybe\"\n");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        UiPrefs::default(),
        "a value this build does not know falls back to the defaults"
    );
    assert!(
        !profile.settings_file().exists(),
        "the broken file is moved aside, not left to be read again"
    );
    assert_eq!(
        fs::read(profile.settings_file().with_extension("toml.bak")).expect("read backup"),
        b"[ui]\nupdate_check = \"maybe\"\n",
        "the unreadable preferences stay in the backup"
    );
}

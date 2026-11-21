//! Layout tests: the `[ui]` table stores the split or unified text layout.

use crate::appearance::{LayoutPref, UiPrefs, load_or_default, save};
use crate::appearance_tests::{TempDir, open_profile, settings_text, write_settings};

/// A `[ui]` table written without the layout key loads as `Split`.
#[test]
fn a_ui_table_without_layout_loads_split() {
    let temp = TempDir::new("layout-missing");
    let profile = open_profile(&temp);
    write_settings(&profile, "[ui]\npalette = \"violet\"\n");
    assert_eq!(
        load_or_default(&profile).expect("load").layout,
        LayoutPref::Split,
        "a table without a layout key falls back to split"
    );
}

/// The unified layout survives a save and load, and the file names its key.
#[test]
fn unified_layout_round_trips() {
    let temp = TempDir::new("layout-unified");
    let profile = open_profile(&temp);
    let prefs = UiPrefs {
        layout: LayoutPref::Unified,
        ..UiPrefs::default()
    };
    save(&profile, &prefs).expect("save");
    assert_eq!(
        load_or_default(&profile).expect("load"),
        prefs,
        "the layout must survive a save and a load"
    );
    assert!(
        settings_text(&profile).contains("layout = \"unified\""),
        "the saved text must name the layout"
    );
}

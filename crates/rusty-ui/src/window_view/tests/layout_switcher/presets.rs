use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::Tiles;

use super::super::{fake, open_options, options_for, press, two_panes, type_in};
use super::{LOGS, REVIEW, current_tiles, shown};
use crate::settings::Values;
use crate::window_view::{FerriteWindow, LayoutEdit, SaveReport};

#[derive(Default)]
struct Disk {
    edits: Vec<LayoutEdit>,
    failure: Option<String>,
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    disk: Arc<Mutex<Disk>>,
}

fn edited(values: &Values, edit: &LayoutEdit) -> Values {
    let mut presets: Vec<(String, String)> = values
        .layout_presets()
        .iter()
        .map(|(name, tiles)| (name.to_string(), tiles.print()))
        .collect();
    match edit {
        LayoutEdit::Save { name, tiles } => presets.push((name.clone(), tiles.clone())),
        LayoutEdit::Replace { name, tiles } => presets
            .iter_mut()
            .filter(|(old, _)| old == name)
            .for_each(|(_, old)| old.clone_from(tiles)),
        LayoutEdit::Remove { name } => presets.retain(|(old, _)| old != name),
    }
    values
        .clone()
        .with_layout_presets(presets.iter().map(|(n, t)| (n.as_str(), t.as_str())))
}

fn rig(cx: &mut TestAppContext, presets: &[(&str, &str)]) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let mut options = options_for(fs, state);
    let values = options
        .settings
        .values
        .clone()
        .with_layout_presets(presets.iter().copied());
    let disk = Arc::new(Mutex::new(Disk::default()));
    let stored = Arc::new(Mutex::new(values.clone()));
    let log = disk.clone();
    options.settings.values = values;
    options.settings.edit_layout = Arc::new(move |edit| {
        let mut disk = log.lock().expect("disk");
        if let Some(error) = disk.failure.clone() {
            return Err(error);
        }
        disk.edits.push(edit.clone());
        let mut stored = stored.lock().expect("stored");
        *stored = edited(&stored, edit);
        Ok(SaveReport {
            generation: disk.edits.len() as u64,
            ..SaveReport::saved(stored.clone())
        })
    });
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "ctrl-l");
    Rig { window, view, disk }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn edits(&self) -> Vec<LayoutEdit> {
        self.disk.lock().expect("disk").edits.clone()
    }

    fn is_naming(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.tiles.presets.is_naming())
    }

    fn asking(&self, cx: &mut TestAppContext) -> Option<LayoutEdit> {
        self.view
            .read_with(cx, |view, _| view.tiles.presets.asking().cloned())
    }

    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.tiles.switcher.is_some())
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice()
                .filter(|notice| notice.is_error)
                .map(|notice| notice.text.to_string())
        })
    }

    fn panes(&self, cx: &mut TestAppContext) -> usize {
        self.view
            .read_with(cx, |view, _| view.tiles_layout().numbered().len())
    }
}

#[gpui_kit::test]
fn ctrl_s_saves_the_current_layout_and_it_appears_as_key_six(cx: &mut TestAppContext) {
    let rig = rig(cx, &[]);
    rig.press(cx, &["v", "ctrl-s"]);
    assert!(rig.is_naming(cx));
    let tiles = current_tiles(cx, &rig.view);
    rig.press(cx, &["enter"]);
    let name = "Layout 1".to_owned();
    let saved = tiles.print();
    assert_eq!(rig.edits(), [LayoutEdit::Save { name, tiles: saved }]);
    assert!(!rig.is_naming(cx));
    assert!(rig.is_open(cx));
    assert!(shown(cx, rig.window, "layout-preset", 6));
    rig.press(cx, &["1", "ctrl-l", "6"]);
    assert_eq!(current_tiles(cx, &rig.view), tiles);
}

#[gpui_kit::test]
fn the_default_name_skips_names_in_use(cx: &mut TestAppContext) {
    let rig = rig(cx, &[("Layout 1", REVIEW), ("Layout 2", LOGS)]);
    rig.press(cx, &["ctrl-s", "enter"]);
    assert!(matches!(
        rig.edits().as_slice(),
        [LayoutEdit::Save { name, .. }] if name == "Layout 3"
    ));
}

#[gpui_kit::test]
fn a_taken_name_asks_before_it_replaces_the_preset(cx: &mut TestAppContext) {
    let rig = rig(cx, &[("Review", REVIEW), ("Logs", LOGS)]);
    let before = current_tiles(cx, &rig.view);
    assert_ne!(before, Tiles::parse(REVIEW).unwrap());
    rig.press(cx, &["ctrl-s"]);
    type_in(cx, rig.window, &rig.view, "Review");
    rig.press(cx, &["enter"]);
    let tiles = current_tiles(cx, &rig.view).print();
    let replace = LayoutEdit::Replace {
        name: "Review".to_owned(),
        tiles,
    };
    assert_eq!(rig.asking(cx), Some(replace.clone()));
    assert!(rig.edits().is_empty());
    rig.press(cx, &["enter"]);
    assert_eq!(rig.edits(), [replace]);
    assert!(shown(cx, rig.window, "layout-preset", 7));
    assert!(!shown(cx, rig.window, "layout-preset", 8));
    rig.press(cx, &["1", "ctrl-l", "6"]);
    assert_eq!(current_tiles(cx, &rig.view), before);
}

#[gpui_kit::test]
fn delete_on_a_user_preset_asks_then_removes_it(cx: &mut TestAppContext) {
    let rig = rig(cx, &[("Review", REVIEW)]);
    assert!(!shown(cx, rig.window, "layout-key", 16));
    rig.press(cx, &["right", "right", "right", "right", "right"]);
    assert!(shown(cx, rig.window, "layout-key", 16));
    rig.press(cx, &["delete"]);
    let remove = LayoutEdit::Remove {
        name: "Review".to_owned(),
    };
    assert_eq!(rig.asking(cx), Some(remove.clone()));
    rig.press(cx, &["enter"]);
    assert!(rig.edits().is_empty());
    rig.press(cx, &["delete", "left", "enter"]);
    assert_eq!(rig.edits(), [remove]);
    assert!(rig.is_open(cx));
    assert!(!shown(cx, rig.window, "layout-preset", 6));
}

#[gpui_kit::test]
fn delete_on_a_built_in_does_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, &[("Review", REVIEW)]);
    let before = current_tiles(cx, &rig.view);
    let panes = rig.panes(cx);
    rig.press(cx, &["delete"]);
    assert_eq!(rig.asking(cx), None);
    assert!(rig.edits().is_empty());
    assert!(rig.is_open(cx));
    assert_eq!(current_tiles(cx, &rig.view), before);
    assert_eq!(rig.panes(cx), panes);
}

#[gpui_kit::test]
fn a_write_error_shows_in_the_status_line_and_changes_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, &[]);
    rig.disk.lock().expect("disk").failure = Some("disk full".to_owned());
    rig.press(cx, &["ctrl-s", "enter"]);
    let notice = rig.notice(cx).expect("notice");
    assert!(notice.contains("disk full"), "{notice}");
    assert!(rig.edits().is_empty());
    assert!(rig.is_open(cx));
    assert!(!shown(cx, rig.window, "layout-preset", 6));
}

#[gpui_kit::test]
fn at_twenty_a_new_name_is_refused_but_a_taken_name_replaces(cx: &mut TestAppContext) {
    let names: Vec<String> = (1..=20).map(|n| format!("Preset {n}")).collect();
    let presets: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), REVIEW)).collect();
    let rig = rig(cx, &presets);
    rig.press(cx, &["ctrl-s", "enter"]);
    assert!(rig.is_naming(cx));
    let error = rig
        .view
        .read_with(cx, |view, _| view.tiles.presets.name_error());
    assert_eq!(
        error.as_deref(),
        Some(rusty_l10n::tr("layout-presets-full").as_str())
    );
    assert!(rig.edits().is_empty());
    type_in(cx, rig.window, &rig.view, "Preset 20");
    rig.press(cx, &["enter"]);
    let replace = LayoutEdit::Replace {
        name: "Preset 20".to_owned(),
        tiles: current_tiles(cx, &rig.view).print(),
    };
    assert_eq!(rig.asking(cx), Some(replace.clone()));
    rig.press(cx, &["enter"]);
    assert_eq!(rig.edits(), [replace]);
}

#[gpui_kit::test]
fn the_dialog_keeps_the_keys_and_escape_closes_it_before_the_switcher(cx: &mut TestAppContext) {
    let rig = rig(cx, &[("Review", REVIEW)]);
    let panes = rig.panes(cx);
    rig.press(cx, &["ctrl-s", "v", "x", "delete"]);
    assert!(rig.is_naming(cx));
    assert_eq!(rig.panes(cx), panes);
    rig.press(cx, &["escape"]);
    assert!(!rig.is_naming(cx));
    assert!(rig.is_open(cx));
    rig.press(cx, &["right", "right", "right", "right", "right", "delete"]);
    assert!(rig.asking(cx).is_some());
    rig.press(cx, &["escape"]);
    assert_eq!(rig.asking(cx), None);
    assert!(rig.is_open(cx));
    rig.press(cx, &["v"]);
    assert_eq!(rig.panes(cx), panes + 1);
    rig.press(cx, &["escape"]);
    assert!(!rig.is_open(cx));
    assert!(rig.edits().is_empty());
}

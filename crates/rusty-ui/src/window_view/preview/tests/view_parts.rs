use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_core::AppState;
use rusty_core::shell_menu::MenuItem;
use rusty_fs::FakeFs;

use super::Rig;
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::preview::browse::{ToggleLargestFiles, ToggleTypeBars};
use crate::window_view::preview::spot::Spot;
use crate::window_view::tests::{open_options, options_for, two_panes};
use crate::window_view::{
    FerriteWindow, SaveReport, SettingsHooks, SettingsNews, SettingsSender, StartOptions,
};

const BARS: Field = Field::PreviewTypeBars;
const LARGEST: Field = Field::PreviewLargestFiles;

struct Parts {
    rig: Rig,
    saved: Arc<Mutex<Vec<Change>>>,
    failure: Arc<Mutex<Option<String>>>,
    news: SettingsSender,
}

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/src/lib.rs", 200);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn off(fields: &[Field]) -> Values {
    fields.iter().fold(Values::default(), |values, field| {
        values.with(&Change {
            field: *field,
            value: Value::Flag(false),
        })
    })
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn parts(cx: &mut TestAppContext, values: Values) -> Parts {
    cx.executor().allow_parking();
    let fs = Arc::new(project());
    let saved: Arc<Mutex<Vec<Change>>> = Arc::default();
    let failure: Arc<Mutex<Option<String>>> = Arc::default();
    let stored = Arc::new(Mutex::new(values.clone()));
    let (log, failing) = (saved.clone(), failure.clone());
    let (settings, news) = SettingsHooks {
        values,
        save: Arc::new(move |changes| {
            let mut stored = stored.lock().expect("stored");
            if let Some(error) = failing.lock().expect("failure").clone() {
                return SaveReport {
                    failed: changes.iter().map(|change| change.field).collect(),
                    error: Some(error),
                    ..SaveReport::saved(stored.clone())
                };
            }
            log.lock().expect("saved").extend_from_slice(changes);
            *stored = changes
                .iter()
                .fold(stored.clone(), |all, one| all.with(one));
            SaveReport::saved(stored.clone())
        }),
        ..SettingsHooks::none()
    }
    .channel();
    let state: AppState = two_panes(&fs);
    let options = StartOptions {
        settings,
        ..options_for(fs, state)
    };
    let (window, view) = open_options(cx, options);
    view.update(cx, |view, _| view.preview.set_every(Duration::ZERO));
    let rig = Rig { window, view };
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    Parts {
        rig,
        saved,
        failure,
        news,
    }
}

impl Parts {
    fn browse(&self, cx: &mut TestAppContext) {
        self.rig.key(cx, "ctrl-shift-q");
    }

    fn settle(&self, cx: &mut TestAppContext) {
        self.rig.settle_motion(cx);
        self.rig.rest(cx);
    }

    fn items(&self, cx: &mut TestAppContext) -> Option<Vec<MenuItem>> {
        self.rig.view.read_with(cx, |view, cx| {
            let menu = view.preview_menu_view()?;
            Some(menu.read(cx).menu().items.clone())
        })
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<(String, bool)> {
        self.items(cx)
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.text, item.checked))
            .collect()
    }

    fn run(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.rig.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
        self.rig.key(cx, "enter");
    }

    fn toggle(&self, cx: &mut TestAppContext, text: &str) {
        self.run(cx, text);
        self.rig.key(cx, "escape");
    }

    fn exists(&self, cx: &mut TestAppContext, id: (&'static str, usize)) -> bool {
        cx.update_window(self.rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some()
        })
        .expect("window")
    }

    fn saved(&self) -> Vec<Change> {
        self.saved.lock().expect("saved").clone()
    }

    fn setting(&self, cx: &mut TestAppContext, field: Field) -> Value {
        self.rig
            .view
            .read_with(cx, |view, _| view.values.get(field).clone())
    }

    fn bars(&self, cx: &mut TestAppContext) -> bool {
        self.exists(cx, ("preview-type-bar", 0))
    }

    fn largest(&self, cx: &mut TestAppContext) -> bool {
        self.exists(cx, ("preview-largest", 0))
    }

    fn spot_is_largest(&self, cx: &mut TestAppContext) -> bool {
        self.rig.read(cx, |view| {
            view.preview
                .folder()
                .is_some_and(|folder| matches!(folder.spot, Some(Spot::Largest(_))))
        })
    }
}

fn menu_rows(bars: bool, largest: bool) -> Vec<(String, bool)> {
    vec![
        ("Type bars".to_owned(), bars),
        ("Largest files".to_owned(), largest),
        (String::new(), false),
        ("Keep as default".to_owned(), false),
        ("Reset to defaults".to_owned(), false),
    ]
}

fn change(field: Field, is_on: bool) -> Change {
    Change {
        field,
        value: Value::Flag(is_on),
    }
}

#[gpui_kit::test]
fn both_parts_show_by_default(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    assert!(parts.bars(cx));
    assert!(parts.largest(cx));
}

#[gpui_kit::test]
fn alt_v_in_the_browsing_sheet_opens_the_preview_menu_with_check_marks(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    assert!(parts.items(cx).is_none());
    parts.rig.key(cx, "alt-v");
    assert_eq!(parts.rows(cx), menu_rows(true, true));
    parts.rig.key(cx, "escape");
    assert!(parts.items(cx).is_none());
    assert!(parts.rig.read(cx, |view| view.preview.is_open()));
}

#[gpui_kit::test]
fn the_view_button_in_the_header_opens_the_same_menu(cx: &mut TestAppContext) {
    let parts = parts(cx, off(&[LARGEST]));
    assert!(parts.rig.shows(cx, "preview-view"));
    parts.rig.click(cx, "preview-view");
    assert_eq!(parts.rows(cx), menu_rows(true, false));
}

#[gpui_kit::test]
fn alt_v_with_the_list_focused_still_opens_the_window_menu(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.rig.key(cx, "alt-v");
    assert!(parts.items(cx).is_none());
    assert!(parts.rig.read(cx, |view| view.window_menu_view().is_some()));
}

#[gpui_kit::test]
fn hiding_the_type_bars_changes_only_the_open_sheet(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    assert_eq!(parts.rows(cx), menu_rows(false, true));
    parts.rig.redraw(cx);
    assert!(!parts.bars(cx));
    assert!(parts.largest(cx));
    assert_eq!(parts.saved(), []);
    assert_eq!(parts.setting(cx, BARS), Value::Flag(true));
    parts.rig.key(cx, "escape");
    parts.rig.key(cx, "escape");
    parts.settle(cx);
    parts.rig.open(cx);
    assert!(parts.rig.wait_until(cx, is_done));
    assert!(parts.bars(cx), "closing the sheet forgets the toggle");
}

#[gpui_kit::test]
fn hiding_the_largest_files_drops_the_section_and_its_rows_from_the_keys(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Largest files");
    assert_eq!(parts.rows(cx), menu_rows(true, false));
    parts.rig.key(cx, "escape");
    parts.rig.redraw(cx);
    assert!(!parts.largest(cx));
    assert!(parts.bars(cx));
    for _ in 0..8 {
        parts.rig.key(cx, "down");
    }
    assert!(!parts.spot_is_largest(cx));
    assert_eq!(parts.saved(), []);
}

#[gpui_kit::test]
fn the_cursor_leaves_a_largest_row_when_the_section_is_hidden(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    for _ in 0..8 {
        parts.rig.key(cx, "down");
    }
    assert!(parts.spot_is_largest(cx));
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Largest files");
    parts.rig.key(cx, "escape");
    assert!(!parts.spot_is_largest(cx));
    parts.rig.key(cx, "down");
    assert!(!parts.spot_is_largest(cx));
}

#[gpui_kit::test]
fn keep_as_default_saves_the_changed_parts_through_the_settings(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    assert!(parts.items(cx).is_none());
    assert_eq!(parts.saved(), [change(BARS, false)]);
    assert_eq!(parts.setting(cx, BARS), Value::Flag(false));
    assert!(!parts.bars(cx));
    assert_eq!(
        parts.rig.label(cx, "side-sheet-status").as_deref(),
        Some("Kept as the default in navo.toml.")
    );
}

#[gpui_kit::test]
fn keep_on_a_broken_settings_file_shows_the_problem_and_keeps_the_toggle(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    *parts.failure.lock().expect("failure") = Some("navo.toml is invalid at line 4.".to_owned());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Largest files");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    let shown = parts.rig.label(cx, "side-sheet-status").unwrap_or_default();
    assert!(shown.contains("navo.toml is invalid at line 4."), "{shown}");
    assert!(!shown.contains("Kept"), "{shown}");
    assert!(!parts.largest(cx));
    assert_eq!(parts.setting(cx, LARGEST), Value::Flag(true));
    *parts.failure.lock().expect("failure") = None;
    parts.rig.key(cx, "alt-v");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    assert_eq!(parts.saved(), [change(LARGEST, false)]);
    assert_eq!(parts.setting(cx, LARGEST), Value::Flag(false));
}

#[gpui_kit::test]
fn reset_to_defaults_brings_the_sheet_back_to_the_settings(cx: &mut TestAppContext) {
    let parts = parts(cx, off(&[BARS]));
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.toggle(cx, "Largest files");
    parts.rig.redraw(cx);
    assert!(parts.bars(cx) && !parts.largest(cx));
    parts.run(cx, "Reset to defaults");
    parts.rig.redraw(cx);
    assert!(parts.items(cx).is_none());
    assert!(!parts.bars(cx) && parts.largest(cx));
    assert_eq!(parts.saved(), []);
}

#[gpui_kit::test]
fn a_settings_change_reaches_the_open_sheet_except_parts_toggled_there(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.rig.key(cx, "escape");
    let values = off(&[LARGEST]);
    assert!(parts.news.send(SettingsNews::Applied {
        values,
        generation: 1,
    }));
    parts.settle(cx);
    assert!(!parts.largest(cx));
    assert!(!parts.bars(cx));
    assert!(parts.news.send(SettingsNews::Applied {
        values: Values::default(),
        generation: 2,
    }));
    parts.settle(cx);
    assert!(parts.largest(cx));
    assert!(!parts.bars(cx), "the sheet's own choice stays");
}

#[gpui_kit::test]
fn a_sheet_opens_with_the_parts_the_settings_ask_for(cx: &mut TestAppContext) {
    let parts = parts(cx, off(&[BARS, LARGEST]));
    assert!(!parts.bars(cx));
    assert!(!parts.largest(cx));
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    assert_eq!(parts.rows(cx), menu_rows(false, false));
    parts.toggle(cx, "Type bars");
    parts.rig.redraw(cx);
    assert!(parts.bars(cx));
}

#[gpui_kit::test]
fn the_other_keys_of_the_sheet_work_with_both_parts_hidden(cx: &mut TestAppContext) {
    let parts = parts(cx, off(&[BARS, LARGEST]));
    parts.browse(cx);
    parts.rig.key(cx, "down");
    let before = parts
        .rig
        .read(cx, |view| view.preview.folder().and_then(|f| f.spot));
    assert!(before.is_some());
    parts.rig.key(cx, "ctrl-r");
    parts.rig.key(cx, "escape");
    assert!(!parts.rig.read(cx, |view| view.preview.is_open()));
}

fn shows_folder(name: &'static str) -> impl Fn(&FerriteWindow) -> bool {
    move |view| {
        is_done(view)
            && view
                .preview
                .folder()
                .is_some_and(|folder| folder.target.name() == name)
    }
}

fn spot_place_is_tree(view: &FerriteWindow) -> bool {
    view.preview.folder().is_some_and(|folder| {
        matches!(folder.spot, Some(Spot::Tree(_))) && folder.place().is_some()
    })
}

#[gpui_kit::test]
fn a_nested_folder_keeps_the_largest_files_hidden(cx: &mut TestAppContext) {
    let parts = parts(cx, off(&[LARGEST]));
    parts.browse(cx);
    parts.rig.key(cx, "enter");
    assert!(parts.rig.wait_until(cx, shows_folder("src")));
    for _ in 0..8 {
        parts.rig.key(cx, "down");
    }
    assert!(!parts.spot_is_largest(cx));
    assert!(parts.rig.read(cx, spot_place_is_tree));
}

#[gpui_kit::test]
fn a_nested_folder_follows_a_largest_files_toggle(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "enter");
    assert!(parts.rig.wait_until(cx, shows_folder("src")));
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Largest files");
    parts.rig.key(cx, "escape");
    for _ in 0..8 {
        parts.rig.key(cx, "down");
    }
    assert!(!parts.spot_is_largest(cx));
}

#[gpui_kit::test]
fn going_up_never_lands_on_a_hidden_largest_row(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    for _ in 0..8 {
        parts.rig.key(cx, "down");
    }
    assert!(parts.spot_is_largest(cx));
    parts.rig.key(cx, "enter");
    parts.settle(cx);
    assert!(parts.news.send(SettingsNews::Applied {
        values: off(&[LARGEST]),
        generation: 1,
    }));
    parts.settle(cx);
    parts.rig.key(cx, "backspace");
    assert!(parts.rig.wait_until(cx, shows_folder("proj")));
    assert!(!parts.spot_is_largest(cx));
    parts.rig.key(cx, "down");
    assert!(!parts.spot_is_largest(cx));
}

fn mark_two(parts: &Parts, cx: &mut TestAppContext) {
    parts.rig.key(cx, "space");
    parts.rig.key(cx, "space");
}

#[gpui_kit::test]
fn a_failed_keep_shows_over_the_marks_summary(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    *parts.failure.lock().expect("failure") = Some("navo.toml is invalid at line 4.".to_owned());
    parts.browse(cx);
    mark_two(&parts, cx);
    let marked = parts.rig.label(cx, "side-sheet-status").unwrap_or_default();
    assert!(marked.contains("2 marked"), "{marked}");
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    let shown = parts.rig.label(cx, "side-sheet-status").unwrap_or_default();
    assert!(shown.contains("Not kept"), "{shown}");
}

#[gpui_kit::test]
fn a_toggle_after_keep_clears_the_kept_note(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    let kept = parts.rig.label(cx, "side-sheet-status").unwrap_or_default();
    assert!(kept.contains("Kept"), "{kept}");
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Largest files");
    parts.rig.redraw(cx);
    assert_eq!(parts.rig.label(cx, "side-sheet-status"), None);
}

#[gpui_kit::test]
fn a_reset_after_keep_clears_the_kept_note(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.run(cx, "Keep as default");
    parts.settle(cx);
    parts.rig.key(cx, "alt-v");
    parts.run(cx, "Reset to defaults");
    parts.rig.redraw(cx);
    assert_eq!(parts.rig.label(cx, "side-sheet-status"), None);
}

#[gpui_kit::test]
fn keep_shows_saving_while_the_save_runs(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    parts.rig.key(cx, "alt-v");
    parts.toggle(cx, "Type bars");
    parts.rig.key(cx, "alt-v");
    cx.update_window(parts.rig.window, |_, window, cx| {
        window.input("Keep as default", cx)
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(parts.rig.window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    let note = parts.rig.read(cx, |view| view.preview.view.note());
    assert_eq!(note.as_deref(), Some("Saving\u{2026}"));
    parts.settle(cx);
    let note = parts.rig.read(cx, |view| view.preview.view.note());
    assert_eq!(note.as_deref(), Some("Kept as the default in navo.toml."));
}

fn dispatch(parts: &Parts, cx: &mut TestAppContext, action: Box<dyn gpui_kit::Action>) {
    cx.update_window(parts.rig.window, |_, window, cx| {
        window.dispatch_action(action, cx)
    })
    .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_type_bars_command_toggles_the_part(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    dispatch(&parts, cx, Box::new(ToggleTypeBars));
    parts.rig.redraw(cx);
    assert!(!parts.bars(cx));
    assert!(parts.largest(cx));
}

#[gpui_kit::test]
fn the_largest_files_command_toggles_the_part(cx: &mut TestAppContext) {
    let parts = parts(cx, Values::default());
    parts.browse(cx);
    dispatch(&parts, cx, Box::new(ToggleLargestFiles));
    parts.rig.redraw(cx);
    assert!(!parts.largest(cx));
    assert!(parts.bars(cx));
}

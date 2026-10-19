use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use image::ImageFormat;
use rusty_core::shell_menu::MenuItem;
use rusty_fs::FakeFs;

use super::image::image_view;
use super::menu::{hint, press_key, run_item};
use super::{Log, Setup, editor_of, hooks, options_with, redraw, visible};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::tests::{open_options, press, settle, viewer_open};
use crate::window_view::viewer::Kinds;
use crate::window_view::viewer::editor::{EditorBody, EditorView};
use crate::window_view::viewer::image::fixtures::{encoded, photo};
use crate::window_view::{FerriteWindow, SaveReport, SettingsHooks, SettingsNews, SettingsSender};

const TEXT: &[u8] = b"one\r\ntwo\nthree\n";
const MEMORY: &[u8] = b":020000040800F2
:10000000000102030405060708090A0B0C0D0E0F78
:10001000101112131415161718191A1B1C1D1E1F68
:10010000202122232425262728292A2B2C2D2E2F77
:0400000508000131BD
:00000001FF
";

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    saved: Arc<Mutex<Vec<Change>>>,
    news: SettingsSender,
}

fn two_files(first: &str, second: &str, len: u64) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{first}"), len);
    fs.add_file(&format!("home/{second}"), len);
    fs.add_folder("dest");
    Arc::new(fs)
}

fn rig_on(cx: &mut TestAppContext, names: (&str, &str), setup: Setup, values: Values) -> Rig {
    let fs = two_files(names.0, names.1, setup.bytes.len() as u64);
    let files = hooks(setup, &Log::default());
    let saved: Arc<Mutex<Vec<Change>>> = Arc::default();
    let stored = Arc::new(Mutex::new(values.clone()));
    let log = saved.clone();
    let (settings, news) = SettingsHooks {
        values,
        save: Arc::new(move |changes| {
            log.lock().expect("saved").extend_from_slice(changes);
            let mut stored = stored.lock().expect("stored");
            *stored = changes
                .iter()
                .fold(stored.clone(), |all, one| all.with(one));
            SaveReport::saved(stored.clone())
        }),
        ..SettingsHooks::none()
    }
    .channel();
    let options = crate::window_view::StartOptions {
        settings,
        ..options_with(fs, files, Kinds::builtin())
    };
    let (window, view) = open_options(cx, options);
    let rig = Rig {
        window,
        view,
        saved,
        news,
    };
    rig.press(cx, "down");
    rig.press(cx, "f3");
    rig.settle(cx);
    rig
}

fn rig(cx: &mut TestAppContext, names: (&str, &str), bytes: &[u8]) -> Rig {
    rig_on(cx, names, Setup::new(bytes.to_vec()), Values::default())
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn settle(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, &self.view);
        redraw(cx, self.window);
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    fn menu_items(&self, cx: &mut TestAppContext) -> Option<Vec<MenuItem>> {
        self.view.read_with(cx, |view, cx| {
            let menu = view.viewer_menu_view()?;
            Some(menu.read(cx).menu().items.clone())
        })
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<(String, bool)> {
        self.menu_items(cx)
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.text, item.checked))
            .collect()
    }

    fn is_checked(&self, cx: &mut TestAppContext, text: &str) -> bool {
        self.rows(cx)
            .into_iter()
            .any(|(row, checked)| row == text && checked)
    }

    fn line_ends(&self, cx: &mut TestAppContext) -> bool {
        let editor = self.editor(cx);
        editor.read_with(cx, |editor, _| editor.marking_state().line_ends)
    }

    fn line_numbers(&self, cx: &mut TestAppContext) -> bool {
        let editor = self.editor(cx);
        editor.read_with(cx, |editor, _| editor.shows_line_numbers())
    }

    fn editor(&self, cx: &mut TestAppContext) -> Entity<EditorView> {
        self.view
            .read_with(cx, |view, _| {
                let body = view.viewer.as_ref()?.body.as_ref()?;
                let editor = body.as_any().downcast_ref::<EditorBody>()?;
                Some(editor.0.clone())
            })
            .expect("the text view is open")
    }

    fn shown_file(&self, cx: &mut TestAppContext) -> String {
        self.view.read_with(cx, |view, _| {
            view.viewer_location()
                .map(|location| location.as_str().to_owned())
                .unwrap_or_default()
        })
    }

    fn saved(&self) -> Vec<Change> {
        self.saved.lock().expect("saved").clone()
    }

    fn setting(&self, cx: &mut TestAppContext, field: Field) -> Value {
        self.view
            .read_with(cx, |view, _| view.values.get(field).clone())
    }
}

fn shows(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn text_rows(numbers: bool, ends: bool) -> Vec<(String, bool)> {
    vec![
        ("Line numbers".to_owned(), numbers),
        ("Line ends".to_owned(), ends),
        (String::new(), false),
        ("Keep as default".to_owned(), false),
        ("Reset to defaults".to_owned(), false),
    ]
}

#[gpui_kit::test]
fn alt_v_opens_the_text_view_menu_with_check_marks_and_key_hints(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    assert!(editor_of(cx, &rig.view).is_some());
    assert!(rig.menu_items(cx).is_none());
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), text_rows(true, false));
    assert_eq!(hint(cx, &rig.view, "Line ends").as_deref(), Some("Alt+L"));
    assert_eq!(hint(cx, &rig.view, "Line numbers"), None);
    press_key(cx, rig.window, "escape");
    assert!(rig.menu_items(cx).is_none());
    assert!(viewer_open(cx, &rig.view));
}

#[gpui_kit::test]
fn a_toggle_keeps_the_menu_open_and_changes_only_the_open_file(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Line ends");
    assert!(rig.is_checked(cx, "Line ends"));
    assert!(rig.line_ends(cx));
    press_key(cx, rig.window, "escape");
    press_key(cx, rig.window, "escape");
    assert!(rig.menu_items(cx).is_none());
    assert_eq!(rig.saved(), []);
    assert_eq!(rig.setting(cx, Field::TextLineEnds), Value::Flag(false));
    rig.press(cx, "n");
    rig.settle(cx);
    assert!(rig.shown_file(cx).ends_with("b.txt"));
    assert!(!rig.line_ends(cx));
}

#[gpui_kit::test]
fn alt_l_goes_through_the_view_parts(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-l");
    assert!(rig.line_ends(cx));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), text_rows(true, true));
}

#[gpui_kit::test]
fn keep_as_default_saves_the_changed_part_through_the_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-l");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    assert!(rig.menu_items(cx).is_none());
    let kept = Change {
        field: Field::TextLineEnds,
        value: Value::Flag(true),
    };
    assert_eq!(rig.saved(), [kept]);
    assert_eq!(rig.setting(cx, Field::TextLineEnds), Value::Flag(true));
    assert!(rig.line_ends(cx));
    rig.press(cx, "n");
    rig.settle(cx);
    assert!(rig.line_ends(cx));
}

#[gpui_kit::test]
fn reset_to_defaults_brings_the_open_file_back_to_the_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-l");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Line numbers");
    assert!(!rig.line_numbers(cx));
    press_key(cx, rig.window, "escape");
    press_key(cx, rig.window, "escape");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Reset to defaults");
    assert!(rig.menu_items(cx).is_none());
    assert!(!rig.line_ends(cx));
    assert!(rig.line_numbers(cx));
    assert_eq!(rig.saved(), []);
}

#[gpui_kit::test]
fn a_settings_change_reaches_the_open_viewer_except_parts_toggled_there(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-l");
    let changed = Values::default()
        .with(&Change {
            field: Field::TextLineNumbers,
            value: Value::Flag(false),
        })
        .with(&Change {
            field: Field::TextLineEnds,
            value: Value::Flag(false),
        });
    assert!(rig.news.send(SettingsNews::Applied {
        values: changed,
        generation: 1,
    }));
    rig.settle(cx);
    assert!(!rig.line_numbers(cx));
    assert!(rig.line_ends(cx));
}

#[gpui_kit::test]
fn line_numbers_off_hides_them_in_the_paged_text_view(cx: &mut TestAppContext) {
    let lines: Vec<u8> = (0..2000)
        .flat_map(|at| format!("line {at}\n").into_bytes())
        .collect();
    let rig = rig_on(
        cx,
        ("a.txt", "b.txt"),
        Setup::huge(lines),
        Values::default(),
    );
    assert!(shows(cx, rig.window, ("paged-line", 0usize)));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), text_rows(true, false));
    run_item(cx, rig.window, "Line numbers");
    assert!(!shows(cx, rig.window, ("paged-line", 0usize)));
}

#[gpui_kit::test]
fn the_hex_view_has_no_view_menu_yet(cx: &mut TestAppContext) {
    let bytes: Vec<u8> = (0..4096).map(|at| (at * 7 % 251) as u8).collect();
    let rig = rig(cx, ("a.bin", "b.bin"), &bytes);
    rig.press(cx, "alt-v");
    assert!(rig.menu_items(cx).is_none());
    assert!(viewer_open(cx, &rig.view));
}

fn details_shown(rig: &Rig, cx: &mut TestAppContext) -> bool {
    let image = image_view(cx, &rig.view).expect("an image is open");
    image.read_with(cx, |image, _| image.info)
}

#[gpui_kit::test]
fn ctrl_i_and_i_show_the_image_details_and_escape_closes_them_first(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.jpg", "b.jpg"), &photo(40, 30, 1));
    assert!(!details_shown(&rig, cx));
    rig.press(cx, "ctrl-i");
    assert!(visible(cx, rig.window, "image-info"));
    rig.press(cx, "escape");
    assert!(!details_shown(&rig, cx));
    assert!(viewer_open(cx, &rig.view));
    rig.press(cx, "i");
    assert!(details_shown(&rig, cx));
    rig.press(cx, "alt-v");
    assert_eq!(
        rig.rows(cx),
        [
            ("Details".to_owned(), true),
            (String::new(), false),
            ("Keep as default".to_owned(), false),
            ("Reset to defaults".to_owned(), false),
        ]
    );
    assert_eq!(hint(cx, &rig.view, "Details").as_deref(), Some("I"));
    press_key(cx, rig.window, "escape");
    rig.press(cx, "escape");
    rig.press(cx, "escape");
    assert!(!viewer_open(cx, &rig.view));
}

#[gpui_kit::test]
fn ctrl_b_does_nothing_in_the_image_view(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.png", "b.png"), &encoded(ImageFormat::Png, 40, 30));
    rig.press(cx, "ctrl-b");
    assert!(!details_shown(&rig, cx));
    assert!(viewer_open(cx, &rig.view));
}

fn automotive_rows(preset: usize, parts: [bool; 4]) -> Vec<(String, bool)> {
    let presets = ["Simple", "Rich", "Custom"]
        .iter()
        .enumerate()
        .map(|(at, label)| ((*label).to_owned(), at == preset));
    let names = ["Open the inspector", "Memory strip", "Charts", "Row bars"];
    let parts = names
        .iter()
        .zip(parts)
        .map(|(label, on)| ((*label).to_owned(), on));
    let tail = [
        (String::new(), false),
        ("Keep as default".to_owned(), false),
        ("Reset to defaults".to_owned(), false),
    ];
    presets
        .chain([(String::new(), false)])
        .chain(parts)
        .chain(tail)
        .collect()
}

#[gpui_kit::test]
fn the_automotive_view_menu_shows_its_preset_and_a_toggle_makes_it_custom(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.hex", "b.hex"), MEMORY);
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), automotive_rows(0, [false; 4]));
    run_item(cx, rig.window, "Rich");
    assert_eq!(rig.rows(cx), automotive_rows(1, [true; 4]));
    press_key(cx, rig.window, "escape");
    run_item(cx, rig.window, "Charts");
    assert_eq!(rig.rows(cx), automotive_rows(2, [true, true, false, true]));
    assert_eq!(rig.saved(), []);
    assert_eq!(rig.setting(cx, Field::AutomotivePreset), Value::Pick(0));
}

#[gpui_kit::test]
fn the_inspector_part_opens_the_inspector_on_pick_in_this_viewer(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.hex", "b.hex"), MEMORY);
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Open the inspector");
    press_key(cx, rig.window, "escape");
    press_key(cx, rig.window, "escape");
    rig.press(cx, "ctrl-b");
    rig.press(cx, "enter");
    rig.settle(cx);
    assert!(visible(cx, rig.window, "memory-inspector"));
}

#[gpui_kit::test]
fn ctrl_b_and_ctrl_i_open_the_memory_drawers_and_escape_closes_the_top_one(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, ("a.hex", "b.hex"), MEMORY);
    rig.press(cx, "ctrl-b");
    rig.settle(cx);
    assert!(visible(cx, rig.window, "memory-navigator"));
    rig.press(cx, "ctrl-i");
    rig.settle(cx);
    assert!(visible(cx, rig.window, "memory-inspector"));
    rig.press(cx, "escape");
    rig.settle(cx);
    assert!(!visible(cx, rig.window, "memory-inspector"));
    assert!(visible(cx, rig.window, "memory-navigator"));
    rig.press(cx, "escape");
    rig.settle(cx);
    assert!(!visible(cx, rig.window, "memory-navigator"));
    assert!(viewer_open(cx, &rig.view));
    rig.press(cx, "escape");
    assert!(!viewer_open(cx, &rig.view));
}

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use image::ImageFormat;

use super::find::bottom_keys;
use super::image::image_view;
use super::menu::{hint, press_key, run_item};
use super::view_rig::{Rig, rig, rig_on, shows};
use super::{Setup, editor_of, label, visible};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::SettingsNews;
use crate::window_view::tests::viewer_open;
use crate::window_view::viewer::image::fixtures::{encoded, photo};

const TEXT: &[u8] = b"one\r\ntwo\nthree\n";
const MEMORY: &[u8] = b":020000040800F2
:10000000000102030405060708090A0B0C0D0E0F78
:10001000101112131415161718191A1B1C1D1E1F68
:10010000202122232425262728292A2B2C2D2E2F77
:0400000508000131BD
:00000001FF
";

fn text_rows(numbers: bool, ends: bool) -> Vec<(String, bool)> {
    vec![
        ("Line numbers".to_owned(), numbers),
        ("Line ends".to_owned(), ends),
        (String::new(), false),
        ("Encoding\u{2026}".to_owned(), false),
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

pub(super) fn note(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("viewer-message")
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

#[gpui_kit::test]
fn keep_on_a_broken_settings_file_shows_the_problem_and_keeps_the_toggle(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    *rig.failure.lock().expect("failure") = Some("ferrite.toml is invalid at line 2.".to_owned());
    rig.press(cx, "alt-l");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    let shown = note(cx, rig.window).unwrap_or_default();
    assert!(
        shown.contains("ferrite.toml is invalid at line 2."),
        "{shown}"
    );
    assert!(!shown.contains("Kept"), "{shown}");
    assert!(rig.line_ends(cx));
    assert_eq!(rig.setting(cx, Field::TextLineEnds), Value::Flag(false));
    *rig.failure.lock().expect("failure") = None;
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    assert_eq!(
        note(cx, rig.window).as_deref(),
        Some("Kept as the default in ferrite.toml.")
    );
    assert_eq!(rig.setting(cx, Field::TextLineEnds), Value::Flag(true));
    assert!(rig.line_ends(cx));
}

#[gpui_kit::test]
fn the_view_key_is_on_the_bar_of_every_viewer_with_parts(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    assert!(bottom_keys(cx, rig.window).contains(&"Alt+V View".to_owned()));
    let bytes: Vec<u8> = (0..4096).map(|at| (at * 7 % 251) as u8).collect();
    let hex = self::rig(cx, ("a.bin", "b.bin"), &bytes);
    assert!(!bottom_keys(cx, hex.window).contains(&"Alt+V View".to_owned()));
}

#[gpui_kit::test]
fn the_image_bar_lists_the_view_key_and_the_details_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.jpg", "b.jpg"), &photo(40, 30, 1));
    let keys = bottom_keys(cx, rig.window);
    assert!(keys.contains(&"Alt+V View".to_owned()), "{keys:?}");
    assert!(keys.contains(&"Ctrl+I Details".to_owned()), "{keys:?}");
}

#[gpui_kit::test]
fn the_line_ends_row_is_left_out_while_editing(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "ctrl-e");
    rig.press(cx, "alt-v");
    let rows = rig.rows(cx);
    assert!(
        !rows.iter().any(|(text, _)| text == "Line ends"),
        "{rows:?}"
    );
    assert!(rows.iter().any(|(text, _)| text == "Line numbers"));
}

#[gpui_kit::test]
fn rich_then_custom_in_the_menu_keeps_every_automotive_part_shown(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.hex", "b.hex"), MEMORY);
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Rich");
    press_key(cx, rig.window, "escape");
    run_item(cx, rig.window, "Custom");
    assert_eq!(rig.rows(cx), automotive_rows(2, [true; 4]));
}

fn table_rows(preset: usize, inspector: bool) -> Vec<(String, bool)> {
    let presets = ["Simple", "Rich", "Custom"]
        .iter()
        .enumerate()
        .map(|(at, label)| ((*label).to_owned(), at == preset));
    presets
        .chain([
            (String::new(), false),
            ("Open the inspector".to_owned(), inspector),
            (String::new(), false),
            ("Keep as default".to_owned(), false),
            ("Reset to defaults".to_owned(), false),
        ])
        .collect()
}

fn layout_rows(preset: usize, on: bool) -> Vec<(String, bool)> {
    let mut rows = table_rows(preset, on);
    let parts = [
        "Memory strip",
        "Used and free bars",
        "Who uses the flash",
        "Row bars",
    ];
    rows.splice(5..5, parts.map(|part| (part.to_owned(), on)));
    rows
}

fn elf_rig(cx: &mut TestAppContext) -> Rig {
    let bytes = rusty_automotive::elf_sample::demo().expect("the demo builds");
    rig(cx, ("a.elf", "b.elf"), &bytes)
}

fn trace_rig(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx, ("a.asc", "b.asc"), super::can_trace::LOG.as_bytes());
    rig.settle(cx);
    rig.settle(cx);
    rig
}

fn map_rig(cx: &mut TestAppContext) -> Rig {
    let rig = rig(
        cx,
        ("a.map", "b.map"),
        crate::window_view::viewer::map::tests::SAMPLE.as_bytes(),
    );
    rig.settle(cx);
    rig.settle(cx);
    rig
}

fn click(rig: &Rig, cx: &mut TestAppContext, id: (&'static str, usize)) {
    cx.update_window(rig.window, |_, window, cx| window.click(id, cx))
        .expect("window");
    rig.settle(cx);
}

fn choose_rich(rig: &Rig, cx: &mut TestAppContext) {
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Rich");
    press_key(cx, rig.window, "escape");
    press_key(cx, rig.window, "escape");
    rig.settle(cx);
    assert!(rig.menu_items(cx).is_none());
    assert!(viewer_open(cx, &rig.view));
}

#[gpui_kit::test]
fn alt_v_opens_the_view_menu_in_the_elf_view(cx: &mut TestAppContext) {
    let rig = elf_rig(cx);
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), layout_rows(0, false));
    run_item(cx, rig.window, "Rich");
    assert_eq!(rig.rows(cx), layout_rows(1, true));
}

#[gpui_kit::test]
fn alt_v_opens_the_view_menu_in_the_can_trace_view(cx: &mut TestAppContext) {
    let rig = trace_rig(cx);
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), table_rows(0, false));
    run_item(cx, rig.window, "Open the inspector");
    assert_eq!(rig.rows(cx), table_rows(2, true));
}

#[gpui_kit::test]
fn alt_v_opens_the_view_menu_in_the_map_view(cx: &mut TestAppContext) {
    let rig = map_rig(cx);
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), layout_rows(0, false));
    run_item(cx, rig.window, "Rich");
    assert_eq!(rig.rows(cx), layout_rows(1, true));
}

#[gpui_kit::test]
fn the_view_menu_header_reads_view(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), TEXT);
    rig.press(cx, "alt-v");
    assert_eq!(label(cx, rig.window, ("menu-row", 0)), "View");
}

#[gpui_kit::test]
fn the_inspector_opens_by_itself_in_rich_when_an_elf_symbol_is_picked(cx: &mut TestAppContext) {
    let rig = elf_rig(cx);
    click(&rig, cx, ("elf-row", 1));
    assert!(!visible(cx, rig.window, "elf-inspector"));
    choose_rich(&rig, cx);
    rig.press(cx, "ctrl-home");
    rig.settle(cx);
    click(&rig, cx, ("elf-row", 2));
    assert!(visible(cx, rig.window, "elf-inspector"));
}

#[gpui_kit::test]
fn the_inspector_opens_by_itself_in_rich_when_a_map_row_is_picked(cx: &mut TestAppContext) {
    let rig = map_rig(cx);
    click(&rig, cx, ("map-row", 1));
    assert!(!visible(cx, rig.window, "map-inspector"));
    choose_rich(&rig, cx);
    click(&rig, cx, ("map-row", 2));
    assert!(visible(cx, rig.window, "map-inspector"));
}

#[gpui_kit::test]
fn the_inspector_opens_by_itself_in_rich_when_a_trace_frame_is_picked(cx: &mut TestAppContext) {
    let rig = trace_rig(cx);
    click(&rig, cx, ("trace-row", 1));
    assert!(!visible(cx, rig.window, "trace-inspector"));
    choose_rich(&rig, cx);
    click(&rig, cx, ("trace-row", 2));
    assert!(visible(cx, rig.window, "trace-inspector"));
}

#[gpui_kit::test]
fn the_elf_and_trace_bars_list_the_view_key(cx: &mut TestAppContext) {
    let elf = elf_rig(cx);
    assert!(bottom_keys(cx, elf.window).contains(&"Alt+V View".to_owned()));
    let trace = trace_rig(cx);
    assert!(bottom_keys(cx, trace.window).contains(&"Alt+V View".to_owned()));
    let map = map_rig(cx);
    assert!(bottom_keys(cx, map.window).contains(&"Alt+V View".to_owned()));
}

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::find::bottom_keys;
use super::structured::open_named;
use super::{Setup, has_editor, label, redraw, text_of, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in};
use crate::window_view::viewer::map::tests::{SAMPLE, probe, sort_by_size_then_finish_parsing};

fn settle_motion(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
}

fn open_text(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(text.as_bytes().to_vec()));
    settle_motion(cx, window, &view);
    settle_motion(cx, window, &view);
    (window, view)
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_text(cx, "app.map", SAMPLE)
}

fn keys(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    keys: &[&str],
) {
    for key in keys {
        press(cx, window, view, key);
    }
    settle_motion(cx, window, view);
}

fn has_row(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> bool {
    cx.update_window(window, |_, window, _| {
        window.try_find(("map-row", row)).is_some()
    })
    .expect("window")
}

fn row(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> String {
    label(cx, window, ("map-row", row))
}

fn first_cell(text: String) -> String {
    text.split(", ").next().unwrap_or_default().to_owned()
}

fn click(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    id: (&'static str, usize),
) {
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .expect("window");
    settle_motion(cx, window, view);
}

#[gpui_kit::test]
fn f3_on_a_tasking_map_shows_the_memory_table_and_a_status(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    assert_eq!(
        row(cx, window, 0),
        "mpe:dsram0, 32,768, 1,024, 31,744, 3.1%"
    );
    assert_eq!(row(cx, window, 1), "mpe:flash0, 65,536, 960, 64,576, 1.4%");
    assert_eq!(
        text_of(cx, window, "map-status"),
        "TASKING · 3 regions · 4 sections · 6 symbols · used 1.9 KB · free 98.1 KB · 1 other part not shown"
    );
    assert_eq!(text_of(cx, window, "map-count"), "3 regions");
}

#[gpui_kit::test]
fn a_map_file_that_is_not_a_tasking_map_opens_as_text(cx: &mut TestAppContext) {
    let gnu = "Memory Configuration\n\nName Origin Length Attributes\nFLASH 0x0 0x1000 xr\n";
    let (window, view) = open_text(cx, "firmware.map", gnu);
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "map-status"));
}

#[gpui_kit::test]
fn a_tasking_text_that_is_not_named_map_opens_as_text(cx: &mut TestAppContext) {
    let (window, view) = open_text(cx, "notes.txt", SAMPLE);
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "map-status"));
}

#[gpui_kit::test]
fn alt_t_switches_between_the_three_tables(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["alt-t"]);
    assert_eq!(
        row(cx, window, 0),
        ".bss.ram (4), 0x70000000, 1,024, mpe:dsram0, mpe:vtc:linear"
    );
    assert_eq!(text_of(cx, window, "map-count"), "4 sections");
    keys(cx, window, &view, &["alt-t"]);
    assert_eq!(
        row(cx, window, 0),
        "rx_buffer, 0x70000000, 1,024, mpe:vtc:linear"
    );
    assert_eq!(text_of(cx, window, "map-count"), "6 symbols");
    keys(cx, window, &view, &["alt-t"]);
    assert!(row(cx, window, 0).starts_with("mpe:dsram0, "));
}

#[gpui_kit::test]
fn clicking_a_table_name_shows_that_table(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    click(cx, window, &view, ("map-table", 2));
    assert!(row(cx, window, 0).starts_with("rx_buffer, "));
    click(cx, window, &view, ("map-table", 0));
    assert!(row(cx, window, 0).starts_with("mpe:dsram0, "));
}

#[gpui_kit::test]
fn sort_keys_and_header_clicks_sort_and_again_reverse(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["alt-t", "alt-t", "ctrl-f3"]);
    assert_eq!(first_cell(row(cx, window, 0)), "engine_init");
    keys(cx, window, &view, &["ctrl-f3"]);
    assert_eq!(first_cell(row(cx, window, 0)), "turn_right");
    keys(cx, window, &view, &["ctrl-f6"]);
    assert_eq!(first_cell(row(cx, window, 0)), "tune_table");
    keys(cx, window, &view, &["ctrl-f6"]);
    assert_eq!(first_cell(row(cx, window, 0)), "rx_buffer");
    click(cx, window, &view, ("map-head", 1));
    assert_eq!(first_cell(row(cx, window, 0)), "rx_buffer");
    click(cx, window, &view, ("map-head", 1));
    assert_eq!(first_cell(row(cx, window, 0)), "tune_table");
    keys(cx, window, &view, &["ctrl-f4"]);
    assert_eq!(first_cell(row(cx, window, 0)), "engine_init");
}

#[gpui_kit::test]
fn the_memory_sort_keys_use_used_percent_for_the_second_key(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["ctrl-f4"]);
    assert_eq!(first_cell(row(cx, window, 0)), "mpe:pflash1");
    keys(cx, window, &view, &["ctrl-f3"]);
    assert_eq!(first_cell(row(cx, window, 0)), "mpe:dsram0");
}

#[gpui_kit::test]
fn every_letter_and_digit_types_into_the_filter_and_escape_clears_it(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["alt-t", "alt-t"]);
    keys(cx, window, &view, &["t", "u", "r", "n", "_"]);
    assert_eq!(first_cell(row(cx, window, 0)), "turn_left");
    assert!(!has_row(cx, window, 2), "two rows match");
    assert!(text_of(cx, window, "filter").starts_with("Filter turn_, 2 items"));
    keys(
        cx,
        window,
        &view,
        &[
            "backspace",
            "backspace",
            "backspace",
            "backspace",
            "backspace",
        ],
    );
    keys(cx, window, &view, &["n", "p", "1", "3", "5"]);
    assert!(text_of(cx, window, "filter").starts_with("Filter np135, 0 items"));
    keys(cx, window, &view, &["escape"]);
    assert!(!visible(cx, window, "filter"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
    assert_eq!(text_of(cx, window, "map-count"), "6 symbols");
}

#[gpui_kit::test]
fn the_navigator_lists_regions_then_objects_largest_first(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["ctrl-b"]);
    assert!(visible(cx, window, "map-navigator"));
    assert_eq!(
        label(cx, window, ("map-item", 0)),
        "All Whole map, 1.9 KB, 3 regions · 3 objects"
    );
    assert_eq!(
        label(cx, window, ("map-item", 2)),
        "Region mpe:flash0, 960 B / 64.0 KB, 1.4% used · 63.1 KB free"
    );
    assert_eq!(
        label(cx, window, ("map-item", 4)),
        "Object comet.o, 1.0 KB, mpe:dsram0"
    );
    assert_eq!(
        label(cx, window, ("map-item", 5)),
        "Object lunar.o, 512 B, mpe:flash0"
    );
    assert_eq!(
        label(cx, window, ("map-item", 6)),
        "Object engine.o, 448 B, mpe:flash0"
    );
}

#[gpui_kit::test]
fn enter_on_a_region_filters_sections_and_symbols_to_it(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["ctrl-b", "down", "down", "enter"]);
    assert_eq!(text_of(cx, window, "map-count"), "3 of 4 sections");
    assert!(row(cx, window, 0).starts_with(".text.boot (1), "));
    assert!(text_of(cx, window, "map-scope").starts_with("Showing region mpe:flash0"));
    keys(cx, window, &view, &["escape", "alt-t"]);
    assert_eq!(text_of(cx, window, "map-count"), "5 of 6 symbols");
    assert!(!visible(cx, window, "map-navigator"));
}

#[gpui_kit::test]
fn enter_on_an_object_filters_to_what_it_contributes(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(
        cx,
        window,
        &view,
        &["ctrl-b", "down", "down", "down", "down", "down", "enter"],
    );
    assert!(text_of(cx, window, "map-scope").starts_with("Showing object obj/lunar.o"));
    assert_eq!(text_of(cx, window, "map-count"), "1 of 4 sections");
    keys(cx, window, &view, &["alt-t"]);
    assert_eq!(first_cell(row(cx, window, 0)), "turn_left");
    assert_eq!(first_cell(row(cx, window, 1)), "turn_right");
    assert!(!has_row(cx, window, 2));
}

#[gpui_kit::test]
fn enter_on_all_clears_the_scope(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(
        cx,
        window,
        &view,
        &["ctrl-b", "down", "enter", "up", "enter"],
    );
    assert!(!visible(cx, window, "map-scope"));
    assert_eq!(text_of(cx, window, "map-count"), "4 sections");
}

#[gpui_kit::test]
fn escape_closes_the_drawer_then_clears_the_filter_then_closes_the_viewer(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["ctrl-b", "down", "enter", "ctrl-b"]);
    keys(cx, window, &view, &["ctrl-b", "escape"]);
    assert!(!visible(cx, window, "map-navigator"));
    assert!(visible(cx, window, "map-scope"));
    keys(cx, window, &view, &["x"]);
    assert!(visible(cx, window, "filter"));
    keys(cx, window, &view, &["escape"]);
    assert!(!visible(cx, window, "filter"));
    assert!(!visible(cx, window, "map-scope"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
    keys(cx, window, &view, &["escape"]);
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_none()));
}

#[gpui_kit::test]
fn the_inspector_shows_a_region_with_its_top_objects(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["down", "enter"]);
    assert!(visible(cx, window, "map-inspector"));
    let text = inspector_text(cx, window);
    assert!(text.contains("Length 65,536 B"), "{text}");
    assert!(text.contains("Used 960 B · 1.4%"), "{text}");
    assert!(text.contains("lunar.o 512 engine.o 448"), "{text}");
}

fn inspector_text(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    text_of(cx, window, "map-inspector-text")
}

#[gpui_kit::test]
fn the_inspector_shows_a_section_with_its_contributions(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["alt-t", "down", "enter"]);
    let text = inspector_text(cx, window);
    assert!(text.contains("Range 0x80000000–0x8000017F"), "{text}");
    assert!(text.contains("Region mpe:flash0"), "{text}");
    assert!(
        text.contains("engine.o .text.start (1) 256 engine.o .text.run (2) 128"),
        "{text}"
    );
}

#[gpui_kit::test]
fn the_inspector_shows_a_symbol_with_its_section_and_object(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(
        cx,
        window,
        &view,
        &["alt-t", "alt-t", "ctrl-f3", "ctrl-home", "down", "enter"],
    );
    assert_eq!(text_of(cx, window, "map-cursor"), "engine_run · 0x80000100");
    let text = inspector_text(cx, window);
    assert!(text.contains("Section .text.boot (1)"), "{text}");
    assert!(text.contains("Object obj/engine.o"), "{text}");
    assert!(text.contains("Input .text.run (2)"), "{text}");
}

#[gpui_kit::test]
fn ctrl_g_goes_to_the_symbol_holding_an_address(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["t", "u"]);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "0x80000344");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(!visible(cx, window, "filter"), "the filter is cleared");
    assert!(!visible(cx, window, "map-go-to"));
    assert_eq!(text_of(cx, window, "map-cursor"), "turn_right · 0x80000300");
    assert!(visible(cx, window, "map-inspector"));
    assert!(inspector_text(cx, window).contains("Went to 0x80000344"));
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "80000500");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(text_of(cx, window, "map-go-to-error").contains("No symbol holds 0x80000500"));
    type_in(cx, window, &view, "x");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(text_of(cx, window, "map-go-to-error").contains("Not a hex address"));
}

#[gpui_kit::test]
fn the_key_bar_offers_the_table_key_and_the_other_views(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    let keys = bottom_keys(cx, window);
    for wanted in [
        "Alt+T Table",
        "Alt+V View",
        "Alt+1 Text",
        "Ctrl+B Navigator",
    ] {
        assert!(keys.iter().any(|key| key == wanted), "{wanted} in {keys:?}");
    }
    assert!(
        !keys.iter().any(|key| key.contains("Next file")),
        "{keys:?}"
    );
}

#[gpui_kit::test]
fn the_drawers_never_cover_the_status_line(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["ctrl-b", "ctrl-i"]);
    let (status, navigator, inspector) = cx
        .update_window(window, |_, window, _| {
            (
                window.find("map-status").bounds(),
                window.find("map-navigator").bounds(),
                window.find("map-inspector").bounds(),
            )
        })
        .expect("window");
    assert!(navigator.bottom() <= status.top());
    assert!(inspector.bottom() <= status.top());
}

#[gpui_kit::test]
fn problems_always_show_in_the_status_line(cx: &mut TestAppContext) {
    let text = SAMPLE.replace("| 896 | 64 | 0 |", "| 800 | 64 | 0 |");
    let (window, view) = open_text(cx, "broken.map", &text);
    let status = text_of(cx, window, "map-status");
    assert!(
        status.contains("1 region where code + data + reserved differ from used"),
        "{status}"
    );
    keys(cx, window, &view, &["ctrl-b", "ctrl-i"]);
    assert!(text_of(cx, window, "map-status").contains("differ from used"));
}

#[gpui_kit::test]
fn alt_1_shows_the_text_and_alt_5_brings_the_map_back(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["alt-1"]);
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "map-status"));
    keys(cx, window, &view, &["alt-5"]);
    assert!(visible(cx, window, "map-status"));
    assert!(!has_editor(cx, &view));
}

#[gpui_kit::test]
fn a_sort_chosen_while_the_map_loads_orders_the_rows_when_it_is_ready(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    sort_by_size_then_finish_parsing(cx, &view);
    settle_motion(cx, window, &view);
    assert_eq!(first_cell(row(cx, window, 0)), "mpe:pflash1");
}

#[gpui_kit::test]
fn a_go_to_note_goes_when_the_filter_moves_the_cursor_to_another_symbol(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "0x80000344");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(inspector_text(cx, window).contains("Went to 0x80000344"));
    keys(cx, window, &view, &["t", "u", "n"]);
    assert_eq!(text_of(cx, window, "map-cursor"), "tune_table · 0x80001000");
    assert!(!inspector_text(cx, window).contains("Went to"));
}

#[gpui_kit::test]
fn ctrl_g_puts_the_navigator_back_on_all(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(
        cx,
        window,
        &view,
        &["ctrl-b", "down", "down", "enter", "escape"],
    );
    assert_eq!(probe(cx, &view).nav_cursor, 2);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "0x80000344");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert_eq!(probe(cx, &view).nav_cursor, 0);
}

#[gpui_kit::test]
fn the_inspection_is_computed_only_while_the_inspector_is_open(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    keys(cx, window, &view, &["down"]);
    assert!(!probe(cx, &view).has_inspection);
    keys(cx, window, &view, &["ctrl-i"]);
    assert!(probe(cx, &view).has_inspection);
    assert!(inspector_text(cx, window).contains("Length 65,536 B"));
}

#[gpui_kit::test]
fn long_problems_keep_their_place_and_the_info_parts_give_way(cx: &mut TestAppContext) {
    let mut text = SAMPLE.replace("| 896 | 64 | 0 |", "| 800 | 64 | 0 |");
    text.push_str("+ Space mpe:wide (MAU = 16bit)\n| Name | Address |\n| bad | 0x1 |\n");
    let (window, _) = open_text(cx, "broken.map", &text);
    let (shown, parts) = cx
        .update_window(window, |_, window, _| {
            let parts: Vec<_> = (0..8usize)
                .filter_map(|at| window.try_find(("map-status-part", at)))
                .map(|part| part.bounds())
                .collect();
            (window.viewport_size().width, parts)
        })
        .expect("window");
    assert_eq!(parts.len(), 8);
    for part in parts {
        assert!(part.right() <= shown, "{part:?} in a window {shown:?} wide");
    }
    assert!(text_of(cx, window, "map-status").starts_with("1 address space"));
}

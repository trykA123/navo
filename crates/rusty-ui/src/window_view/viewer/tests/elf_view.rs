use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, point, px};
use rusty_automotive::elf_sample::{build, demo, firmware};

use super::find::bottom_keys;
use super::structured::open_named;
use super::{Setup, has_editor, label, redraw, text_of, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in};

fn open_bytes(
    cx: &mut TestAppContext,
    name: &str,
    bytes: Vec<u8>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(bytes));
    settle_motion(cx, window, &view);
    settle_motion(cx, window, &view);
    (window, view)
}

fn has_row(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> bool {
    cx.update_window(window, |_, window, _| {
        window.try_find(("elf-row", row)).is_some()
    })
    .expect("window")
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_bytes(cx, "app.elf", demo().expect("the demo builds"))
}

fn settle_motion(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
}

fn typed(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    text: &str,
) {
    for key in text.chars() {
        press(cx, window, view, &key.to_string());
    }
    settle_motion(cx, window, view);
}

#[gpui_kit::test]
fn f3_on_an_elf_shows_symbols_by_address_and_a_status(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    assert_eq!(
        label(cx, window, ("elf-row", 0)),
        "Reset_Handler, 0x00008001, 64, func, global, .text"
    );
    let status = text_of(cx, window, "elf-status");
    assert_eq!(
        status,
        "ARM · ELF32 little-endian · entry 0x00008001 · load 7.0 KB · RAM 5.0 KB · 32 symbols"
    );
}

#[gpui_kit::test]
fn the_magic_wins_over_the_extension(cx: &mut TestAppContext) {
    let bytes = build(&firmware(false, true)).expect("the sample builds");
    let (window, _) = open_bytes(cx, "firmware.bin", bytes);
    assert!(text_of(cx, window, "elf-status").contains("ELF32 big-endian"));
    let (window, view) = open_bytes(cx, "notes.out", b"build finished\n".to_vec());
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "elf-status"));
}

#[gpui_kit::test]
fn typing_filters_by_name_and_escape_clears(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    typed(cx, window, &view, "can_main");
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Can_MainFunction_Read, "));
    assert!(label(cx, window, ("elf-row", 1)).starts_with("Can_MainFunction_Write, "));
    assert!(!has_row(cx, window, 2), "no third row");
    assert!(text_of(cx, window, "filter").starts_with("Filter can_main, 2 items"));
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Reset_Handler, "));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
}

#[gpui_kit::test]
fn sort_keys_sort_and_again_reverse(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    assert_eq!(
        text_of(cx, window, "elf-cursor"),
        "Reset_Handler · 0x00008001"
    );
    press(cx, window, &view, "ctrl-f6");
    settle_motion(cx, window, &view);
    press(cx, window, &view, "ctrl-home");
    assert!(label(cx, window, ("elf-row", 0)).starts_with("EcuM_State, "));
    press(cx, window, &view, "ctrl-f3");
    settle_motion(cx, window, &view);
    press(cx, window, &view, "ctrl-home");
    assert!(label(cx, window, ("elf-row", 0)).starts_with("bcm::Doors::scan(unsigned int), "));
    press(cx, window, &view, "ctrl-f3");
    settle_motion(cx, window, &view);
    press(cx, window, &view, "ctrl-home");
    assert!(label(cx, window, ("elf-row", 0)).starts_with("WdgM_MainFunction, "));
    press(cx, window, &view, "ctrl-f5");
    settle_motion(cx, window, &view);
    assert_eq!(
        text_of(cx, window, "elf-cursor"),
        "WdgM_MainFunction · 0x00009301",
        "the cursor stays on its symbol"
    );
}

#[gpui_kit::test]
fn drawers_start_closed_open_on_their_keys_and_escape_closes_the_top_one(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    assert!(!visible(cx, window, "elf-navigator"));
    assert!(!visible(cx, window, "elf-inspector"));
    press(cx, window, &view, "ctrl-b");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "elf-navigator"));
    assert!(
        label(cx, window, ("elf-item", 1)).starts_with(".text, 6.0 KB, 0x00008000–0x000097FF · AX")
    );
    press(cx, window, &view, "ctrl-i");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "elf-inspector"));
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "elf-navigator"));
    assert!(
        !visible(cx, window, "elf-inspector"),
        "the inspector opened last"
    );
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    assert!(!visible(cx, window, "elf-navigator"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
}

#[gpui_kit::test]
fn picking_a_section_shows_only_its_symbols(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "ctrl-b");
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(label(cx, window, ("elf-row", 0)).ends_with(".data"));
    assert!(label(cx, window, ("elf-row", 3)).ends_with(".data"));
    assert!(!has_row(cx, window, 4));
}

#[gpui_kit::test]
fn the_inspector_names_the_source_line(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "elf-inspector"));
    assert_eq!(
        text_of(cx, window, "elf-inspector-source"),
        "/work/src/main.c:57"
    );
}

#[gpui_kit::test]
fn the_drawers_never_cover_the_status_line(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "ctrl-b");
    press(cx, window, &view, "ctrl-i");
    settle_motion(cx, window, &view);
    let (status, navigator, inspector) = cx
        .update_window(window, |_, window, _| {
            (
                window.find("elf-status").bounds(),
                window.find("elf-navigator").bounds(),
                window.find("elf-inspector").bounds(),
            )
        })
        .expect("window");
    assert!(
        navigator.bottom() <= status.top(),
        "{navigator:?} {status:?}"
    );
    assert!(
        inspector.bottom() <= status.top(),
        "{inspector:?} {status:?}"
    );
}

#[gpui_kit::test]
fn lister_letters_and_digits_type_into_an_empty_filter(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    typed(cx, window, &view, "pwm_init");
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
    assert!(text_of(cx, window, "filter").starts_with("Filter pwm_init, 1 item"));
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    typed(cx, window, &view, "1ms");
    assert!(text_of(cx, window, "filter").starts_with("Filter 1ms, 1 item"));
    assert!(label(cx, window, ("elf-row", 0)).starts_with("Os_Task_1ms, "));
}

#[gpui_kit::test]
fn the_key_bar_offers_alt_keys_and_no_next_file(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    let keys = bottom_keys(cx, window);
    assert!(keys.iter().any(|key| key == "Alt+1 Text"), "{keys:?}");
    assert!(
        !keys.iter().any(|key| key.contains("Next file")),
        "{keys:?}"
    );
}

#[gpui_kit::test]
fn typing_with_the_navigator_focused_does_not_filter_the_table(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "ctrl-b");
    typed(cx, window, &view, "can");
    assert!(!visible(cx, window, "filter"));
}

#[gpui_kit::test]
fn ctrl_g_goes_to_the_symbol_holding_an_address_and_its_line(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    typed(cx, window, &view, "can");
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "8115");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(
        !visible(cx, window, "filter"),
        "the filter is cleared to show it"
    );
    assert_eq!(text_of(cx, window, "elf-cursor"), "SystemInit · 0x00008101");
    assert!(visible(cx, window, "elf-inspector"));
    assert_eq!(
        text_of(cx, window, "elf-inspector-source"),
        "/work/src/main.c:57"
    );
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "zz");
    press(cx, window, &view, "enter");
    assert!(text_of(cx, window, "elf-go-to-error").contains("Not a hex address"));
}

#[gpui_kit::test]
fn a_file_that_fails_to_parse_says_why_in_the_status(cx: &mut TestAppContext) {
    let mut bytes = build(&firmware(false, false)).expect("the sample builds");
    bytes.truncate(24);
    let (window, _) = open_bytes(cx, "broken.elf", bytes);
    let status = text_of(cx, window, "elf-status");
    assert!(status.contains("not a readable ELF file"), "{status}");
}

fn head_width(cx: &mut TestAppContext, window: AnyWindowHandle, at: usize) -> f32 {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        f32::from(
            window
                .within("elf-head")
                .find(("column", at))
                .bounds()
                .size
                .width,
        )
    })
    .expect("window")
}

#[gpui_kit::test]
fn dragging_a_head_handle_widens_the_column_and_the_width_stays(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    let before = head_width(cx, window, 1);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .within("elf-head")
            .find(("column-handle", 1usize))
            .bounds()
            .center();
        window.drag(from, point(from.x + px(40.), from.y), cx);
    })
    .expect("window");
    settle_motion(cx, window, &view);
    assert!((head_width(cx, window, 1) - before - 40.).abs() <= 1.);
    press(cx, window, &view, "down");
    settle_motion(cx, window, &view);
    assert!((head_width(cx, window, 1) - before - 40.).abs() <= 1.);
}

#[gpui_kit::test]
fn a_head_click_sorts_the_symbols_by_that_column(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("elf-head")
            .click(("column-title", 0usize), cx);
    })
    .expect("window");
    settle_motion(cx, window, &view);
    press(cx, window, &view, "ctrl-home");
    assert!(label(cx, window, ("elf-row", 0)).starts_with("bcm::Doors::scan(unsigned int), "));
}

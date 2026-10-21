use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_automotive::elf_sample::{Named, build, demo, firmware, func};

use crate::window_view::FerriteWindow;
use crate::window_view::tests::press;
use crate::window_view::viewer::map::tests::SAMPLE;
use crate::window_view::viewer::tests::elf_rich::{open_in, preset, settle_all};
use crate::window_view::viewer::tests::{label, map_rich, text_of, visible};

type Rig = (AnyWindowHandle, Entity<FerriteWindow>);

fn rich_elf(cx: &mut TestAppContext) -> Rig {
    rich_elf_of(cx, demo().expect("the demo builds"))
}

fn rich_elf_of(cx: &mut TestAppContext, bytes: Vec<u8>) -> Rig {
    let (window, view) = open_in(cx, bytes, Some(preset(1)));
    settle_all(cx, window, &view);
    (window, view)
}

fn keys(cx: &mut TestAppContext, (window, view): &Rig, keys: &[&str]) {
    for key in keys {
        press(cx, *window, view, key);
    }
    settle_all(cx, *window, view);
}

fn hints(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        (0usize..24)
            .filter_map(|at| Some(bar.try_find(("fkey", at))?.label()?.to_owned()))
            .collect::<Vec<_>>()
            .join(" | ")
    })
    .expect("window")
}

fn picked(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    text_of(cx, window, "elf-inspector-name")
}

fn filter(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    visible(cx, window, "filter").then(|| text_of(cx, window, "filter"))
}

fn is_on(cx: &mut TestAppContext, window: AnyWindowHandle, id: (&'static str, usize)) -> bool {
    cx.update_window(window, |_, window, _| {
        window.find(id).selected() == Some(true)
    })
    .expect("window")
}

#[gpui_kit::test]
fn tab_cycles_table_strips_treemap_and_shift_tab_goes_back(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    assert!(hints(cx, window).contains("Parts"));
    keys(cx, &rig, &["tab"]);
    assert!(
        hints(cx, window).contains("Region"),
        "{}",
        hints(cx, window)
    );
    keys(cx, &rig, &["right"]);
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["tab", "right"]);
    assert_eq!(picked(cx, window), ".data");
    keys(cx, &rig, &["tab"]);
    assert!(hints(cx, window).contains("Select"));
    keys(cx, &rig, &["tab"]);
    assert!(hints(cx, window).contains("Sort by name"));
    keys(cx, &rig, &["shift-tab"]);
    assert!(hints(cx, window).contains("Select"));
    keys(cx, &rig, &["shift-tab", "shift-tab", "end"]);
    assert_eq!(picked(cx, window), ".data");
    assert!(text_of(cx, window, "elf-status").starts_with(".data"));
}

#[gpui_kit::test]
fn arrows_move_the_selection_and_the_inspector_follows(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["tab", "right"]);
    assert!(visible(cx, window, "elf-inspector"));
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["right"]);
    assert_eq!(picked(cx, window), ".data");
    keys(cx, &rig, &["left"]);
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["end"]);
    assert_eq!(picked(cx, window), ".data");
    keys(cx, &rig, &["home"]);
    assert_eq!(picked(cx, window), ".text");
    keys(cx, &rig, &["down", "end"]);
    assert_eq!(picked(cx, window), ".bss");
    keys(cx, &rig, &["down", "home"]);
    assert_eq!(picked(cx, window), ".data", "down stays on the last strip");
    keys(cx, &rig, &["up", "end"]);
    assert_eq!(picked(cx, window), ".data");
    assert!(text_of(cx, window, "elf-status").starts_with(".data"));
}

#[gpui_kit::test]
fn enter_zooms_into_the_block_and_backspace_zooms_out(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["tab", "right"]);
    assert!(!hints(cx, window).contains("Unzoom"));
    keys(cx, &rig, &["enter"]);
    assert!(hints(cx, window).contains("Unzoom"));
    keys(cx, &rig, &["backspace"]);
    assert!(!hints(cx, window).contains("Unzoom"));
    assert_eq!(filter(cx, window), None);
}

#[gpui_kit::test]
fn backspace_on_a_strip_that_is_not_zoomed_edits_the_filter(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["tab", "o", "s"]);
    assert!(filter(cx, window).is_some_and(|text| text.starts_with("Filter os,")));
    keys(cx, &rig, &["backspace"]);
    assert!(filter(cx, window).is_some_and(|text| text.starts_with("Filter o,")));
}

#[gpui_kit::test]
fn backspace_on_the_table_still_edits_the_filter(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["o", "s"]);
    assert!(filter(cx, window).is_some_and(|text| text.starts_with("Filter os,")));
    keys(cx, &rig, &["backspace"]);
    assert!(filter(cx, window).is_some_and(|text| text.starts_with("Filter o,")));
    keys(cx, &rig, &["backspace"]);
    assert_eq!(filter(cx, window), None);
}

#[gpui_kit::test]
fn letters_still_type_into_the_filter(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    keys(cx, &rig, &["g", "z", "h"]);
    assert!(filter(cx, window).is_some_and(|text| text.starts_with("Filter gzh,")));
}

#[gpui_kit::test]
fn the_treemap_arrows_move_between_tiles_and_enter_selects_the_group(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    let first = label(cx, window, ("treemap-tile", 0));
    let name = first.split(", ").next().expect("a name").to_owned();
    keys(cx, &rig, &["tab", "tab", "tab", "right", "left", "enter"]);
    assert_eq!(picked(cx, window), name);
    assert!(text_of(cx, window, "elf-status").starts_with(&name));
}

#[gpui_kit::test]
fn alt_g_moves_to_the_next_grouping_that_can_be_used(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    let before: Vec<bool> = (0..4)
        .map(|at| is_on(cx, window, ("space-grouping", at)))
        .collect();
    keys(cx, &rig, &["alt-g"]);
    let after: Vec<bool> = (0..4)
        .map(|at| is_on(cx, window, ("space-grouping", at)))
        .collect();
    assert_ne!(before, after);
    assert_eq!(after.iter().filter(|on| **on).count(), 1);
}

#[gpui_kit::test]
fn alt_z_switches_compress_gaps_and_true_scale(cx: &mut TestAppContext) {
    let rig = rich_elf(cx);
    let window = rig.0;
    assert!(is_on(cx, window, ("space-scale", 0)));
    keys(cx, &rig, &["alt-z"]);
    assert!(is_on(cx, window, ("space-scale", 1)));
    keys(cx, &rig, &["tab", "alt-z"]);
    assert!(is_on(cx, window, ("space-scale", 0)));
}

#[gpui_kit::test]
fn alt_h_hides_the_noise_symbols(cx: &mut TestAppContext) {
    let mut spec = firmware(false, false);
    spec.symbols.as_mut().expect("symbols").push(Named {
        bind: 0,
        ..func("local_label", 0x0800_0170, 0, 0)
    });
    let rig = rich_elf_of(cx, build(&spec).expect("the sample builds"));
    let window = rig.0;
    assert!(text_of(cx, window, "elf-noise-bar").starts_with("1 of "));
    keys(cx, &rig, &["alt-h"]);
    assert!(text_of(cx, window, "elf-noise-bar").starts_with("0 of "));
}

#[gpui_kit::test]
fn the_map_view_takes_the_same_keys(cx: &mut TestAppContext) {
    let (window, view) = map_rich::open_in(cx, SAMPLE, (None, Some(map_rich::preset(1))));
    let rig = (window, view);
    keys(cx, &rig, &["tab"]);
    assert!(hints(cx, window).contains("Region"));
    keys(cx, &rig, &["right"]);
    assert!(text_of(cx, window, "map-inspector-name").starts_with('.'));
    keys(cx, &rig, &["alt-z"]);
    assert!(is_on(cx, window, ("space-scale", 1)));
}

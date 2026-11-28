//! Ctrl+K offers a viewer's view toggles only while a kind that owns them is open.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Role, TestAppContext};

use super::Setup;
use super::structured::open_named;
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::map::tests::SAMPLE;

const TOGGLES: [&str; 6] = [
    "view.opens_inspector",
    "view.memory_strip",
    "view.charts",
    "view.row_bars",
    "view.usage_bars",
    "view.who_uses",
];

fn toggles(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<&'static str> {
    view.read_with(cx, |view, _| {
        view.palette_entries()
            .iter()
            .filter(|entry| entry.in_palette)
            .map(|entry| entry.spec.name)
            .filter(|name| TOGGLES.contains(name))
            .collect()
    })
}

fn rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let center = window.within("command-center");
        (0usize..)
            .map_while(|index| center.try_find(("command-center-row", index)))
            .filter(|row| row.role() != Some(Role::Heading))
            .filter_map(|row| row.label().map(str::to_owned))
            .collect()
    })
    .expect("window")
}

fn ask_for_memory_strip(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> Vec<String> {
    press(cx, window, view, "ctrl-k");
    cx.update_window(window, |_, window, cx| window.input(">memory strip", cx))
        .expect("window");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(cx, view);
    rows(cx, window)
}

#[gpui_kit::test]
fn a_linker_map_offers_its_view_toggles_in_the_palette(cx: &mut TestAppContext) {
    let (_, view, _) = open_named(cx, "app.map", Setup::new(SAMPLE.as_bytes().to_vec()));
    assert_eq!(
        toggles(cx, &view),
        [
            "view.opens_inspector",
            "view.memory_strip",
            "view.usage_bars",
            "view.who_uses",
            "view.row_bars",
        ]
    );
}

#[gpui_kit::test]
fn a_plain_text_file_offers_no_view_toggles(cx: &mut TestAppContext) {
    let (_, view, _) = open_named(cx, "notes.txt", Setup::new(b"hello\n".to_vec()));
    assert_eq!(toggles(cx, &view), Vec::<&str>::new());
}

#[gpui_kit::test]
fn ctrl_k_in_a_map_viewer_lists_its_memory_strip_toggle(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "app.map", Setup::new(SAMPLE.as_bytes().to_vec()));
    assert_eq!(
        ask_for_memory_strip(cx, window, &view),
        ["Show or hide the memory strip"]
    );
}

#[gpui_kit::test]
fn ctrl_k_in_a_plain_text_viewer_lists_no_memory_strip_toggle(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "notes.txt", Setup::new(b"hello\n".to_vec()));
    assert_eq!(
        ask_for_memory_strip(cx, window, &view),
        Vec::<String>::new()
    );
}

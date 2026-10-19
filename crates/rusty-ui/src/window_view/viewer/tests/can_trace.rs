use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::structured::{fake_with, open_named};
use super::{
    Log, Setup, has_editor, hooks, label, press_only, redraw, text_of, visible, window_with,
};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::{FileHooks, Kinds};

const LOG: &str = "\
date Mon Oct 19 09:41:07.000 am 2026
base hex  timestamps absolute
Begin Triggerblock Mon Oct 19 09:41:07.000 am 2026
   0.000000 Start of measurement
   0.010000 1  123             Rx   d 2 01 02
   0.020000 1  7DF             Rx   d 1 AA
   0.030000 1  123             Rx   d 2 03 04
   0.035000 2  ErrorFrame
   0.040000 1  123             Rx   d 2 05 06
End TriggerBlock
";

fn open(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(text.as_bytes().to_vec()));
    settle_all(cx, window, &view);
    (window, view)
}

fn settle_all(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    for _ in 0..3 {
        settle(cx, view);
        redraw(cx, window);
    }
}

fn settle_motion(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle_all(cx, window, view);
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

#[gpui_kit::test]
fn f3_on_an_asc_log_shows_frames_in_a_table_with_a_status_line(cx: &mut TestAppContext) {
    let (window, _) = open(cx, "bus.asc", LOG);
    assert!(label(cx, window, ("trace-row", 0)).ends_with("Other        Start of measurement"));
    assert_eq!(
        label(cx, window, ("trace-row", 1)),
        "    0.010000  1   123         Rx    CAN       2  01 02"
    );
    assert!(label(cx, window, ("trace-row", 4)).contains("Error"));
    assert_eq!(
        text_of(cx, window, "trace-status"),
        "5 frames · 40.000 ms · 2 channels · 1 error frame"
    );
}

#[gpui_kit::test]
fn an_asc_file_that_is_not_a_trace_opens_as_text(cx: &mut TestAppContext) {
    let armoured =
        "-----BEGIN PGP SIGNATURE-----\n\niQEzBAEBCAAdFiEE\n-----END PGP SIGNATURE-----\n";
    let (window, view) = open(cx, "release.asc", armoured);
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "can-trace"));
}

#[gpui_kit::test]
fn picking_an_id_filters_the_table_and_esc_closes_the_drawer_then_the_filter(
    cx: &mut TestAppContext,
) {
    let (window, view) = open(cx, "bus.asc", LOG);
    assert!(!visible(cx, window, "trace-navigator"));
    keys(cx, window, &view, &["ctrl-b"]);
    assert_eq!(label(cx, window, ("trace-id", 0)), "All rows, 6");
    assert_eq!(label(cx, window, ("trace-id", 1)), "123 · ch 1, 3");
    keys(cx, window, &view, &["down", "enter"]);
    let status = text_of(cx, window, "trace-status");
    assert!(
        status.starts_with("ID 123 · ch 1: 3 frames · Esc shows all"),
        "{status}"
    );
    assert!(label(cx, window, ("trace-row", 1)).contains("03 04"));
    keys(cx, window, &view, &["escape"]);
    assert!(!visible(cx, window, "trace-navigator"));
    assert!(text_of(cx, window, "trace-status").starts_with("ID 123"));
    keys(cx, window, &view, &["escape"]);
    assert!(text_of(cx, window, "trace-status").starts_with("5 frames"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
}

#[gpui_kit::test]
fn typing_in_the_navigator_filters_the_id_list(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "bus.asc", LOG);
    keys(cx, window, &view, &["ctrl-b", "7", "d"]);
    assert_eq!(text_of(cx, window, "trace-id-filter"), "filter: 7d");
    assert_eq!(label(cx, window, ("trace-id", 1)), "7DF · ch 1, 1");
    let has_third = cx
        .update_window(window, |_, window, _| {
            window.try_find(("trace-id", 2usize)).is_some()
        })
        .expect("window");
    assert!(!has_third);
    keys(
        cx,
        window,
        &view,
        &["backspace", "backspace", "1", "2", "enter"],
    );
    assert!(text_of(cx, window, "trace-status").starts_with("ID 123 · ch 1: 3 frames"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
}

#[gpui_kit::test]
fn the_inspector_shows_the_time_since_the_previous_frame_of_the_same_id(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "bus.asc", LOG);
    keys(cx, window, &view, &["down", "down", "down", "ctrl-i"]);
    assert!(visible(cx, window, "trace-inspector"));
    assert_eq!(text_of(cx, window, "trace-delta"), "+20.000 ms");
    keys(cx, window, &view, &["up", "up"]);
    assert_eq!(text_of(cx, window, "trace-delta"), "first frame of this ID");
}

#[gpui_kit::test]
fn drawers_cover_the_table_but_never_the_status_line(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "bus.asc", LOG);
    keys(cx, window, &view, &["ctrl-b", "ctrl-i"]);
    let (navigator, inspector, status) = cx
        .update_window(window, |_, window, _| {
            (
                window.find("trace-navigator").bounds(),
                window.find("trace-inspector").bounds(),
                window.find("trace-status").bounds(),
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
    keys(cx, window, &view, &["escape"]);
    assert!(visible(cx, window, "trace-navigator"));
    assert!(
        !visible(cx, window, "trace-inspector"),
        "the inspector opened last"
    );
}

#[gpui_kit::test]
fn follow_appends_new_frames_and_keeps_the_last_one_in_view(cx: &mut TestAppContext) {
    let first = "base hex  timestamps absolute\n0.1 1 10 Rx d 1 01\n0.2 1 10 Rx d 1 02\n";
    let bytes = Arc::new(Mutex::new(first.as_bytes().to_vec()));
    let files = growing_hooks(first, &bytes);
    let (window, view) = window_with(
        cx,
        fake_with("live.asc", first.len() as u64),
        files,
        Kinds::builtin(),
    );
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle_all(cx, window, &view);
    keys(cx, window, &view, &["alt-f"]);
    assert!(text_of(cx, window, "trace-status").ends_with("following"));
    bytes
        .lock()
        .expect("bytes")
        .extend_from_slice(b"0.3 1 10 Rx d 1 03\n0.4 1 11 Rx d 1 04\n");
    settle_motion(cx, window, &view);
    settle_motion(cx, window, &view);
    assert!(text_of(cx, window, "trace-status").starts_with("4 frames"));
    assert_eq!(text_of(cx, window, "trace-cursor"), "row 4 of 4");
    assert!(label(cx, window, ("trace-row", 3)).ends_with("04"));
}

fn growing_hooks(first: &str, bytes: &Arc<Mutex<Vec<u8>>>) -> FileHooks {
    let base = hooks(Setup::new(first.as_bytes().to_vec()), &Log::default());
    let source = bytes.clone();
    FileHooks {
        range: Arc::new(move |_, offset, len| {
            let bytes = source.lock().expect("bytes");
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
        }),
        ..base
    }
}

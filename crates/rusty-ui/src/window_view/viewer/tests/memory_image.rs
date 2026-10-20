use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, px, size,
};

use super::structured::open_named;
use super::{Setup, has_editor, label, press_only, redraw, text_of, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in};
use crate::window_view::viewer::{FileHooks, Kinds};

const EXAMPLE: &str = "\
:10010000214601360121470136007EFE09D2190140\r
:100110002146017E17C20001FF5F16002148011928\r
:10012000194E79234623965778239EDA3F01B2CAA7\r
:100130003F0156702B5E712B722B732146013421C7\r
:00000001FF\r
";

const WITH_GAP: &str = "\
:020000040800F2
:10000000000102030405060708090A0B0C0D0E0F78
:10001000101112131415161718191A1B1C1D1E1F68
:10020000202122232425262728292A2B2C2D2E2F76
:0400000508000131BD
:00000001FF
";

fn open(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(text.as_bytes().to_vec()));
    settle(cx, &view);
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    (window, view)
}

const FOLDED: &str = "\
:020000040800F2
:10000000000102030405060708090A0B0C0D0E0F78
:10001000101112131415161718191A1B1C1D1E1F68
:10010000202122232425262728292A2B2C2D2E2F77
:00000001FF
";

fn open_sized(
    cx: &mut TestAppContext,
    text: &str,
    width: f32,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view) = open(cx, "app.hex", text);
    resize(cx, window, &view, width);
    (window, view)
}

fn resize(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    width: f32,
) {
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(width), px(600.)));
    for _ in 0..3 {
        settle(cx, view);
        redraw(cx, window);
    }
}

fn settle_motion(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
}

#[gpui_kit::test]
fn f3_on_intel_hex_shows_bytes_on_their_real_addresses(cx: &mut TestAppContext) {
    let (window, _) = open(cx, "app.hex", EXAMPLE);
    let first = label(cx, window, ("memory-row", 0));
    assert!(
        first.starts_with("0x00000100  21 46 01 36 01 21 47 01  36 00 7E FE"),
        "{first}"
    );
    assert_eq!(
        text_of(cx, window, "memory-status"),
        "1 range · 0 gaps · checksum OK · EOF present · no start address"
    );
}

#[gpui_kit::test]
fn a_gap_folds_into_one_row_and_a_bad_checksum_shows_in_the_status(cx: &mut TestAppContext) {
    let broken = WITH_GAP.replacen("0E0F78", "0E0F79", 1);
    let (window, _) = open(cx, "app.hex", &broken);
    assert_eq!(
        label(cx, window, ("memory-gap", 2)),
        "gap 0x08000020–0x080001FF · 480 B not in file"
    );
    assert_eq!(
        text_of(cx, window, "memory-status"),
        "2 ranges · 1 gap · 1 checksum error · EOF present · start 0x08000131"
    );
}

#[gpui_kit::test]
fn a_hex_file_that_is_not_intel_hex_opens_as_text(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "notes.hex", "color: #ff8800\nsize: 12\n");
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "memory-image"));
}

#[gpui_kit::test]
fn s19_files_open_as_a_memory_image(cx: &mut TestAppContext) {
    let (window, _) = open(
        cx,
        "boot.s19",
        "S1130000000102030405060708090A0B0C0D0E0F74\nS9030000FC\n",
    );
    assert!(label(cx, window, ("memory-row", 0)).starts_with("0x00000000  00 01 02"));
}

#[gpui_kit::test]
fn both_drawers_start_closed_and_open_on_their_keys(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.hex", WITH_GAP);
    assert!(!visible(cx, window, "memory-navigator"));
    assert!(!visible(cx, window, "memory-inspector"));
    press(cx, window, &view, "ctrl-b");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "memory-navigator"));
    assert!(label(cx, window, ("memory-item", 0)).starts_with("Range 1, 0x08000000–0x0800001F"));
    press(cx, window, &view, "ctrl-i");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "memory-inspector"));
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "memory-navigator"));
    assert!(
        !visible(cx, window, "memory-inspector"),
        "the inspector opened last"
    );
    press(cx, window, &view, "escape");
    settle_motion(cx, window, &view);
    assert!(!visible(cx, window, "memory-navigator"));
    assert!(view.read_with(cx, |view, _| view.viewer_location().is_some()));
    assert!(label(cx, window, ("memory-row", 0)).starts_with("0x08000000"));
}

#[gpui_kit::test]
fn enter_in_the_navigator_jumps_to_the_picked_range(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.hex", WITH_GAP);
    press(cx, window, &view, "ctrl-b");
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000200 · Range 2");
    assert!(!visible(cx, window, "memory-inspector"));
}

#[gpui_kit::test]
fn the_inspector_opens_by_itself_when_the_setting_asks(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.hex", WITH_GAP);
    let rich = crate::settings::Values::default().with(&crate::settings::Change {
        field: crate::settings::Field::AutomotivePreset,
        value: crate::settings::Value::Pick(1),
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.set_values(rich, window, cx));
    })
    .expect("window");
    press(cx, window, &view, "ctrl-b");
    press(cx, window, &view, "enter");
    settle_motion(cx, window, &view);
    assert!(visible(cx, window, "memory-inspector"));
}

#[gpui_kit::test]
fn ctrl_g_goes_to_an_address_and_rejects_one_outside_the_image(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.hex", WITH_GAP);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "8000205");
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000200 · Range 2");
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "100");
    press(cx, window, &view, "enter");
    let error = text_of(cx, window, "memory-go-to-error");
    assert!(error.contains("outside the image"), "{error}");
}

fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(id).bounds()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_status_line_stays_visible_with_both_drawers_open(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.hex", WITH_GAP);
    press(cx, window, &view, "ctrl-b");
    press(cx, window, &view, "ctrl-i");
    settle_motion(cx, window, &view);
    let status = bounds_of(cx, window, "memory-status");
    for drawer in ["memory-navigator", "memory-inspector"] {
        let sheet = bounds_of(cx, window, drawer);
        assert!(
            !sheet.intersects(&status),
            "{drawer} {sheet:?} covers {status:?}"
        );
    }
}

#[gpui_kit::test]
fn a_failed_row_read_is_shown_once_and_not_retried_on_every_frame(cx: &mut TestAppContext) {
    let bytes = WITH_GAP.as_bytes().to_vec();
    let calls = Arc::new(AtomicUsize::new(0));
    let base = super::hooks(Setup::new(bytes.clone()), &super::Log::default());
    let counted = calls.clone();
    let files = FileHooks {
        range: Arc::new(move |_, offset, len| {
            if counted.fetch_add(1, Ordering::Relaxed) > 0 {
                return Err("The drive was removed.".to_owned());
            }
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
        }),
        ..base
    };
    let fs = super::structured::fake_with("app.hex", WITH_GAP.len() as u64);
    let (window, view) = super::window_with(cx, fs, files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    for _ in 0..5 {
        settle(cx, &view);
        redraw(cx, window);
    }
    assert!(calls.load(Ordering::Relaxed) <= 3, "{calls:?}");
    let status = text_of(cx, window, "memory-status");
    assert!(status.contains("The drive was removed."), "{status}");
}

#[gpui_kit::test]
fn a_wide_window_shows_32_bytes_per_row_and_a_narrow_one_16(cx: &mut TestAppContext) {
    let (window, view) = open_sized(cx, WITH_GAP, 2400.);
    let wide = label(cx, window, ("memory-row", 0));
    assert!(wide.contains("1E 1F"), "{wide}");
    assert!(wide.contains("0F  10"), "{wide}");
    resize(cx, window, &view, 600.);
    let narrow = label(cx, window, ("memory-row", 0));
    assert!(narrow.contains("0E 0F"), "{narrow}");
    assert!(!narrow.contains("1E 1F"), "{narrow}");
    assert!(label(cx, window, ("memory-row", 1)).starts_with("0x08000010"));
}

#[gpui_kit::test]
fn resizing_keeps_the_cursor_on_its_address(cx: &mut TestAppContext) {
    let (window, view) = open_sized(cx, WITH_GAP, 600.);
    press(cx, window, &view, "down");
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000010 · Range 1");
    resize(cx, window, &view, 2400.);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000000 · Range 1");
    resize(cx, window, &view, 600.);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000000 · Range 1");
}

#[gpui_kit::test]
fn a_short_padding_gap_folds_into_the_rows_and_is_counted(cx: &mut TestAppContext) {
    let (window, view) = open_sized(cx, FOLDED, 600.);
    assert_eq!(
        text_of(cx, window, "memory-status"),
        "2 ranges · 1 gap · 1 folded · checksum OK · EOF present · no start address"
    );
    let folded = label(cx, window, ("memory-row", 2));
    assert!(folded.starts_with("0x08000020  --"), "{folded}");
    let gap_rows = cx
        .update_window(window, |_, window, _| {
            window.try_find(("memory-gap", 2usize)).is_some()
        })
        .expect("window");
    assert!(!gap_rows, "no gap row for a folded gap");
    press(cx, window, &view, "ctrl-b");
    settle_motion(cx, window, &view);
    assert!(label(cx, window, ("memory-item", 0)).contains("1 padding gap"));
}

#[gpui_kit::test]
fn go_to_inside_a_folded_gap_lands_on_the_row_that_holds_it(cx: &mut TestAppContext) {
    let (window, view) = open_sized(cx, FOLDED, 600.);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "8000055");
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000050 · Range 1");
}

#[gpui_kit::test]
fn go_to_inside_a_long_gap_lands_on_the_gap_row(cx: &mut TestAppContext) {
    let (window, view) = open_sized(cx, WITH_GAP, 600.);
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "8000100");
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "memory-cursor"), "0x08000020 · Gap");
    assert_eq!(
        label(cx, window, ("memory-gap", 2)),
        "gap 0x08000020–0x080001FF · 480 B not in file"
    );
}

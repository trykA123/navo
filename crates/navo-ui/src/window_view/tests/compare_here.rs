//! Show bytes here (V41d): a line over 1 MiB offers the switch to Bytes at its own offset on a
//! text pair and on a large pair, and Show text returns to the line.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, TestAppContext,
};
use navo_l10n::tr;

use super::compare_hex::{frame, hex_rig, probe};
use super::compare_large::open_large_in;
use super::compare_screen::Rig;
use super::compare_screen::body::pair;
use super::compare_switch::{LINE, open};
use super::screen_access::screen_read;
use crate::commands::compare::{ShowBytesHere, ViewText};
use crate::compare_diff::{DiffRow, DiffUpdate, LONG_LINE, Layout, PairKind, PairText};

/// The line (and list row) that is over 1 MiB.
const AT: usize = 300;

/// A 400-line pair with one change on line 300, whose left line is over 1 MiB; line `n`
/// starts at byte `16 * n` on both sides.
fn long_text() -> PairText {
    let starts: Vec<u64> = (0..=400).map(|line| line * LINE).collect();
    let mut text = PairText {
        left_starts: starts.clone(),
        right_starts: starts,
        ..pair(400, &[AT as u32])
    };
    text.left[AT] = "x".repeat(LONG_LINE + 1);
    text
}

/// Whether the element `id` is drawn.
fn has(cx: &mut TestAppContext, rig: &Rig, id: (&'static str, usize)) -> bool {
    cx.update_window(rig.window, |_, window, _| window.try_find(id).is_some())
        .expect("window")
}

/// Clicks the element `id` and draws.
fn click(cx: &mut TestAppContext, rig: &Rig, id: (&'static str, usize)) {
    cx.update_window(rig.window, |_, window, cx| window.click(id, cx))
        .expect("window");
    frame(cx, rig);
}

/// A rig with 8 KiB sides and `text` opened as a text pair and scrolled to its change.
pub(super) fn text_rig(cx: &mut TestAppContext, text: PairText) -> Rig {
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Text), Some(text));
    rig.press(cx, "alt-down");
    rig
}

/// The rig with the long text pair.
pub(super) fn long_rig(cx: &mut TestAppContext) -> Rig {
    text_rig(cx, long_text())
}

#[gpui_kit::test]
fn compare_here_a_text_line_over_one_mebibyte_opens_bytes_at_its_offset_and_text_returns(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = long_rig(cx);
    assert!(
        has(cx, &rig, ("compare-bytes-left", AT)),
        "the long line offers it"
    );
    for other in [
        ("compare-bytes-left", AT - 1),
        ("compare-bytes-left", AT + 1),
    ] {
        assert!(
            !has(cx, &rig, other),
            "a short line offers nothing: {other:?}"
        );
    }
    assert!(
        !has(cx, &rig, ("compare-bytes-right", AT)),
        "the right line is short"
    );
    click(cx, &rig, ("compare-bytes-left", AT));
    assert_eq!(rig.diff.views(), vec![(2, true)], "one forced bytes reload");
    assert!(
        has(cx, &rig, ("compare-bytes-left", AT)),
        "the text stays drawn meanwhile"
    );
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(probe(cx, &rig).built.start, AT, "row n holds byte 16 * n");
    rig.dispatch(cx, &ViewText);
    open(cx, &rig, (3, PairKind::Text), Some(long_text()));
    let (_, built, _) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(built[0], AT, "the long line is on top again: {built:?}");
}

#[gpui_kit::test]
fn compare_here_a_one_sided_right_line_opens_bytes_at_its_right_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let mut text = long_text();
    let long = std::mem::take(&mut text.left[AT]);
    (text.right[AT], text.rows[AT]) = (long, DiffRow::Inserted(AT as u32));
    text.right_starts[AT] = 0x1000;
    let rig = text_rig(cx, text);
    assert!(has(cx, &rig, ("compare-bytes-right", AT)));
    assert!(!has(cx, &rig, ("compare-bytes-left", AT)));
    click(cx, &rig, ("compare-bytes-right", AT));
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(
        probe(cx, &rig).built.start,
        0x100,
        "right byte 0x1000 is row 0x100"
    );
}

/// The long row as a change whose left line stays short: only the right side is over 1 MiB.
fn right_long_text() -> PairText {
    let mut text = long_text();
    text.left[AT] = "short".to_owned();
    text.right[AT] = "x".repeat(LONG_LINE + 1);
    text.right_starts[AT] = 0x1000;
    text
}

#[gpui_kit::test]
fn compare_here_a_changed_row_with_a_long_right_line_lands_on_the_right_offset(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = text_rig(cx, right_long_text());
    assert!(
        has(cx, &rig, ("compare-bytes-right", AT)),
        "the long right line offers it"
    );
    assert!(
        !has(cx, &rig, ("compare-bytes-left", AT)),
        "the short left line offers nothing"
    );
    click(cx, &rig, ("compare-bytes-right", AT));
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(
        probe(cx, &rig).built.start,
        0x100,
        "the right byte 0x1000 is row 0x100, not the left line's row {AT}"
    );
}

/// Lines `0..40` of request `request` with line 5 over 1 MiB; line `n` starts at byte `16 * n`.
fn large_lines(request: u64) -> DiffUpdate {
    let side = |name: &str| -> Vec<String> {
        (0..40)
            .map(|at| match (name, at) {
                ("l", 5) => "x".repeat(LONG_LINE + 1),
                _ => format!("{name}{at}"),
            })
            .collect()
    };
    let starts: Vec<u64> = (0..40).map(|at| at * LINE).collect();
    DiffUpdate::LargeLines {
        request,
        rows: 0..40,
        left: side("l"),
        right: side("r"),
        left_cut: false,
        right_cut: false,
        left_starts: starts.clone(),
        right_starts: starts,
        totals: (40, 40),
    }
}

#[gpui_kit::test]
fn compare_here_a_large_line_over_one_mebibyte_opens_bytes_and_text_returns(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    let sink = open_large_in(cx, &rig);
    super::compare_large::push(cx, &rig, &sink, large_lines(1));
    frame(cx, &rig);
    assert!(
        has(cx, &rig, ("compare-bytes-left", 5)),
        "the long line offers it"
    );
    assert!(
        !has(cx, &rig, ("compare-bytes-left", 4)),
        "a short line does not"
    );
    click(cx, &rig, ("compare-bytes-left", 5));
    assert_eq!(rig.diff.views(), vec![(2, true)], "one forced bytes reload");
    assert!(
        has(cx, &rig, ("compare-bytes-left", 5)),
        "the large body stays drawn meanwhile"
    );
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(probe(cx, &rig).built.start, 5, "row n holds byte 16 * n");
    rig.dispatch(cx, &ViewText);
    assert_eq!(rig.diff.views(), vec![(2, true), (3, false)]);
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 3,
        kind: PairKind::Large {
            left: 1 << 30,
            right: 1 << 30,
        },
        text: None,
    });
    super::compare_large::push(cx, &rig, &sink, large_lines(3));
    let probe = screen_read(&rig.view, cx, |view, _| view.large_probe()).expect("a large body");
    assert_eq!(probe.top, 5, "the long line is on top again");
}

/// Moves the pointer onto the element `id` and draws.
fn hover(cx: &mut TestAppContext, rig: &Rig, id: (&'static str, usize)) {
    cx.update_window(rig.window, |_, window, cx| window.hover(id, cx))
        .expect("window");
    frame(cx, rig);
}

/// Right-clicks the middle of the element `id` and draws.
fn right_click(cx: &mut TestAppContext, rig: &Rig, id: (&'static str, usize)) {
    cx.update_window(rig.window, |_, window, cx| {
        let position = window.find(id).bounds().center();
        let event = MouseDownEvent {
            button: MouseButton::Right,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        };
        window.dispatch_event(event.to_platform_input(), cx);
    })
    .expect("window");
    frame(cx, rig);
}

/// Whether the compare menu is open.
fn menu_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    screen_read(&rig.view, cx, |view, _| view.compare_menu_view().is_some())
}

/// The last note as (text, has Undo, is an error).
fn note(cx: &mut TestAppContext, rig: &Rig) -> Option<(String, bool, bool)> {
    screen_read(&rig.view, cx, |view, _| view.copy_note())
}

/// The long row as a one-sided insert of a copyable change, so its hover bar and the
/// "Show bytes here" button sit on the same row.
fn insert_text() -> PairText {
    let mut text = long_text();
    text.hunks[0].digest = Some([7; 16]);
    let long = std::mem::take(&mut text.left[AT]);
    (text.right[AT], text.rows[AT]) = (long, DiffRow::Inserted(AT as u32));
    text.right_starts[AT] = 0x1000;
    text
}

#[gpui_kit::test]
fn compare_here_the_hover_bar_leaves_the_show_bytes_here_button_free(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = text_rig(cx, insert_text());
    hover(cx, &rig, ("compare-file-row", AT));
    assert!(
        has(cx, &rig, ("compare-copy-right", AT)),
        "the change carries the bar"
    );
    click(cx, &rig, ("compare-bytes-right", AT));
    assert!(
        rig.diff.copies().is_empty(),
        "a click at the button's centre did not copy"
    );
    assert_eq!(
        rig.diff.views(),
        vec![(2, true)],
        "the click ran Show bytes here"
    );
}

#[gpui_kit::test]
fn compare_here_show_bytes_here_without_a_long_line_says_so(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = long_rig(cx);
    rig.dispatch(cx, &ShowBytesHere);
    let said = tr("compare-bytes-here-none");
    assert_eq!(
        note(cx, &rig),
        Some((said, false, true)),
        "the notice says why"
    );
}

#[gpui_kit::test]
fn compare_here_a_right_click_on_a_large_row_opens_the_menu_with_show_bytes_here(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    let sink = open_large_in(cx, &rig);
    super::compare_large::push(cx, &rig, &sink, large_lines(1));
    frame(cx, &rig);
    right_click(cx, &rig, ("row", 5));
    assert!(menu_open(cx, &rig), "the menu opens on a large row");
    let labels = screen_read(&rig.view, cx, |view, _| view.file_menu_labels());
    assert!(
        labels.contains(&tr("cmd-compare-show-bytes-here")),
        "the long line lists Show bytes here: {labels:?}"
    );
    assert!(
        !labels.contains(&tr("compare-menu-copy-left")),
        "copy change cannot work on Large: {labels:?}"
    );
    rig.press(cx, "escape");
    right_click(cx, &rig, ("row", 4));
    let labels = screen_read(&rig.view, cx, |view, _| view.file_menu_labels());
    assert!(
        !labels.contains(&tr("cmd-compare-show-bytes-here")),
        "a short line does not: {labels:?}"
    );
}

#[gpui_kit::test]
fn compare_here_show_bytes_here_on_a_large_pair_aims_at_the_first_long_line_in_view(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    let sink = open_large_in(cx, &rig);
    super::compare_large::push(cx, &rig, &sink, large_lines(1));
    frame(cx, &rig);
    rig.dispatch(cx, &ShowBytesHere);
    assert_eq!(rig.diff.views(), vec![(2, true)], "one forced bytes reload");
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(probe(cx, &rig).built.start, 5, "row n holds byte 16 * n");
}

/// `request`'s lines `0..40` with none over 1 MiB.
fn short_lines(request: u64) -> DiffUpdate {
    let side = |name: &str| -> Vec<String> { (0..40).map(|at| format!("{name}{at}")).collect() };
    let starts: Vec<u64> = (0..40).map(|at| at * LINE).collect();
    DiffUpdate::LargeLines {
        request,
        rows: 0..40,
        left: side("l"),
        right: side("r"),
        left_cut: false,
        right_cut: false,
        left_starts: starts.clone(),
        right_starts: starts,
        totals: (40, 40),
    }
}

#[gpui_kit::test]
fn compare_here_show_bytes_here_on_a_large_pair_without_a_long_line_says_so(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    let sink = open_large_in(cx, &rig);
    super::compare_large::push(cx, &rig, &sink, short_lines(1));
    frame(cx, &rig);
    rig.dispatch(cx, &ShowBytesHere);
    let said = tr("compare-bytes-here-none");
    assert_eq!(
        note(cx, &rig),
        Some((said, false, true)),
        "the notice says why"
    );
}

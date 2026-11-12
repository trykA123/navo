//! Wrapped rows in the file diff (D39.5–D39.6): one list item per visual line, a row as tall as
//! its taller side, Alt+↓ through the index, toggles that keep the top pair row, and a resize.
//! The map jump, the Alt+V seek, the pick reveal and a centre held over a rebuilt index all work
//! through the same index.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Pixels, TestAppContext, VisualTestContext, px, size};

use super::super::compare_hex::hex_rig;
use super::super::compare_switch::{LINE, open};
use super::body::pair;
use super::{Rig, rig, rig_sized};
use crate::commands::compare::{NextDifference, ViewBytes, ViewText};
use crate::compare_diff::{DiffUpdate, Layout, PairKind, PairText, WrapIndex};

/// The wrap index, its width in columns, the top list item and the items the list has room for.
type Seen = (Option<WrapIndex>, Option<u32>, usize, usize);

/// A pair of `len` rows changed at `changed`, whose every right line is `cols` columns wide.
fn wrapped_pair(len: u32, changed: &[u32], cols: usize) -> PairText {
    let mut text = pair(len, changed);
    text.right = vec!["w".repeat(cols); len as usize];
    text
}

/// Lets the background width measure land, then draws three frames: the probe, the index, and
/// the scroll it asked for.
fn frames(rig: &Rig, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
}

/// Opens `text` and draws until the widths and the text cell are known.
fn opened(rig: &Rig, cx: &mut TestAppContext, text: PairText) {
    rig.open_text(cx, text);
    frames(rig, cx);
}

/// Turns wrap on or off through the look, as Alt+Z will, and redraws.
fn set_wrap(rig: &Rig, cx: &mut TestAppContext, wrap: bool) {
    rig.view.update(cx, |view, _| {
        view.file_look_mut().expect("open").wrap = wrap;
    });
    frames(rig, cx);
}

fn row_height(cx: &mut TestAppContext) -> Pixels {
    cx.update(|cx| crate::tokens::current(cx).spacing.row_height)
}

fn seen(rig: &Rig, cx: &mut TestAppContext) -> Seen {
    let height = row_height(cx);
    rig.view
        .update(cx, |view, _| view.file_wrap(height))
        .expect("an opened text pair")
}

/// The index and its width, once wrap is on and everything is measured.
fn index(rig: &Rig, cx: &mut TestAppContext) -> (WrapIndex, u32) {
    let (index, cols, ..) = seen(rig, cx);
    (index.expect("a wrap index"), cols.expect("a wrap width"))
}

/// A pair like `wrapped_pair`, whose left lines each start `LINE` bytes apart, so the Alt+V
/// switch can map its top line to a byte offset and back.
fn bytes_pair(len: u32, changed: &[u32], cols: usize) -> PairText {
    PairText {
        left_starts: (0..=u64::from(len)).map(|line| line * LINE).collect(),
        ..wrapped_pair(len, changed, cols)
    }
}

/// Whether list item `item` is on screen: at or below the top item, and no further down than the
/// room of whole rows plus the partial row the list still shows at its bottom edge.
fn in_view(top: usize, room: usize, item: usize) -> bool {
    top <= item && item <= top + room
}

/// Whether list item `item` sits in the middle of a list showing `room` items from `top`: at
/// most half a screen below the top, and at least a third, so a row left on top reads as wrong.
fn centred(top: usize, room: usize, item: usize) -> bool {
    let below = item.saturating_sub(top);
    below <= room / 2 + 1 && below * 3 >= room
}

/// The pair row the list's top item belongs to, once wrap is on and the index is built.
fn top_row(rig: &Rig, cx: &mut TestAppContext) -> usize {
    let (list, _) = index(rig, cx);
    list.at(seen(rig, cx).2).0
}

/// `text` with copyable changes: the pick keys only reach the lines of a change that has a digest.
fn copyable(mut text: PairText) -> PairText {
    for hunk in &mut text.hunks {
        hunk.digest = Some([1; 16]);
    }
    text
}

#[gpui_kit::test]
fn a_huge_wrapped_pair_builds_only_the_items_in_view(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(200_000, &[], 300));
    set_wrap(&rig, cx, true);
    let (index, cols) = index(&rig, cx);
    let lines = 300usize.div_ceil(cols as usize);
    assert!(lines > 1, "300 columns wrap at {cols}");
    assert_eq!(index.total(), 200_000 * lines);
    let in_view = seen(&rig, cx).3;
    let built = rig.probe(cx, Layout::SideBySide).1;
    assert!(!built.is_empty());
    assert!(
        built.len() <= in_view / lines + 2,
        "{} pair rows for {in_view} items of {lines} lines",
        built.len()
    );
}

#[gpui_kit::test]
fn a_tall_row_takes_the_items_of_its_taller_side(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    let mut text = pair(40, &[3]);
    text.left[3] = "l".repeat(30);
    text.right[3] = "r".repeat(400);
    opened(&rig, cx, text);
    set_wrap(&rig, cx, true);
    let (index, cols) = index(&rig, cx);
    let lines = 400u32.div_ceil(cols);
    assert_eq!(index.lines_of(3), lines, "the right side sets the height");
    assert_eq!((index.lines_of(2), index.lines_of(4)), (1, 1));
    assert_eq!(index.total(), 39 + lines as usize);
    assert_eq!(index.first_of(4), 4 + lines as usize - 1);
}

#[gpui_kit::test]
fn alt_down_puts_the_first_item_of_the_change_in_view(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(400, &[10, 200, 390], 300));
    set_wrap(&rig, cx, true);
    rig.press(cx, "f7");
    rig.press(cx, "alt-down");
    frames(&rig, cx);
    let (index, _) = index(&rig, cx);
    let (_, _, top, in_view) = seen(&rig, cx);
    let first = index.first_of(200);
    assert!(
        top <= first && first < top + in_view,
        "item {first} in {top}..{}",
        top + in_view
    );
    assert!(rig.probe(cx, Layout::SideBySide).1.contains(&200));
}

/// The VM shape (W8o): two long changed lines among short ones; once change 1 centres at the
/// top, change 2's first item is already fully visible at the bottom edge, and Alt+↓ must
/// still centre it instead of leaving the change hanging below the viewport.
#[gpui_kit::test]
fn alt_down_centres_a_change_whose_first_item_is_already_visible(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(6, &[1, 4], 10));
    set_wrap(&rig, cx, true);
    let (_, cols) = index(&rig, cx);
    let room = seen(&rig, cx).3;
    assert!(room >= 10, "the rig shows {room} items");
    let wide = cols as usize * (room - 5);
    let mut text = pair(6, &[1, 4]);
    for at in [1, 4] {
        text.left[at] = "l".repeat(wide);
        text.right[at] = "r".repeat(wide);
    }
    open(cx, &rig, (2, PairKind::Text), Some(text));
    frames(&rig, cx);
    let (index, _) = index(&rig, cx);
    let lines = index.lines_of(1) as usize;
    assert_eq!(lines, room - 5, "the long rows wrap to {lines} items");
    rig.press(cx, "f7");
    frames(&rig, cx);
    let first = index.first_of(4);
    let (_, _, top, room) = seen(&rig, cx);
    assert_eq!(top, 0, "change 1 centres at the top of the short list");
    assert!(
        in_view(top, room, first),
        "change 2's first item is on screen"
    );
    assert!(
        first + lines > top + room,
        "the rest of change 2 is below it"
    );
    rig.press(cx, "alt-down");
    frames(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    assert_eq!(
        rig.probe(cx, Layout::SideBySide).0,
        Some(1),
        "change 2 of 2"
    );
    assert!(
        centred(top, room, first),
        "change 2 centres: item {first} at top {top}, room {room}"
    );
}

#[gpui_kit::test]
fn toggling_wrap_keeps_the_top_pair_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(400, &[200], 300));
    rig.press(cx, "alt-down");
    frames(&rig, cx);
    let flat_top = seen(&rig, cx).2;
    assert!(flat_top > 100, "scrolled to the change: top {flat_top}");
    set_wrap(&rig, cx, true);
    let (index, _) = index(&rig, cx);
    let top = seen(&rig, cx).2;
    assert_eq!(index.at(top), (flat_top, 0), "wrap on keeps the top row");
    set_wrap(&rig, cx, false);
    assert_eq!(seen(&rig, cx).2, flat_top, "wrap off keeps it too");
}

#[gpui_kit::test]
fn a_narrower_window_wraps_at_fewer_columns(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig_sized(cx, px(1400.));
    opened(&rig, cx, wrapped_pair(40, &[3], 300));
    set_wrap(&rig, cx, true);
    let (_, wide) = index(&rig, cx);
    VisualTestContext::from_window(rig.window, cx).simulate_resize(size(px(1000.), px(800.)));
    frames(&rig, cx);
    let (index, narrow) = index(&rig, cx);
    assert!(narrow < wide, "{narrow} columns after {wide}");
    assert_eq!(index.total(), 40 * 300usize.div_ceil(narrow as usize));
}

#[gpui_kit::test]
fn a_map_click_with_tall_rows_above_centres_its_row(cx: &mut TestAppContext) {
    use crate::compare_diff::map::MapEvent;

    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(400, &[10, 200, 390], 300));
    set_wrap(&rig, cx, true);
    let (index, _) = index(&rig, cx);
    let map = rig
        .view
        .read_with(cx, |view, _| view.file_map())
        .expect("a mounted change map");
    map.update(cx, |_, cx| cx.emit(MapEvent::Jump(200)));
    frames(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    let first = index.first_of(200);
    assert!(
        in_view(top, room, first),
        "the clicked row is in view: item {first}, list {top}..{}",
        top + room
    );
    assert!(
        centred(top, room, first),
        "and centred, not on top: item {first} at top {top}, room {room}"
    );
}

#[gpui_kit::test]
fn alt_v_to_bytes_and_back_keeps_the_top_pair_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    rig.press(cx, "enter");
    open(
        cx,
        &rig,
        (1, PairKind::Text),
        Some(bytes_pair(400, &[10, 200, 390], 300)),
    );
    set_wrap(&rig, cx, true);
    rig.press(cx, "f7");
    rig.press(cx, "alt-down");
    frames(&rig, cx);
    let top = top_row(&rig, cx);
    assert!(top > 100, "the second change is on screen: top row {top}");
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    rig.dispatch(cx, &ViewText);
    open(
        cx,
        &rig,
        (3, PairKind::Text),
        Some(bytes_pair(400, &[10, 200, 390], 300)),
    );
    frames(&rig, cx);
    assert_eq!(
        top_row(&rig, cx),
        top,
        "the round trip keeps the top pair row"
    );
}

#[gpui_kit::test]
fn a_pick_under_a_tall_row_and_its_menu_anchor_reach_the_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, copyable(wrapped_pair(400, &[390], 300)));
    set_wrap(&rig, cx, true);
    let (index, _) = index(&rig, cx);
    rig.press(cx, "shift-down");
    frames(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    let first = index.first_of(390);
    assert!(
        in_view(top, room, first),
        "the picked row is in view: item {first}, list {top}..{}",
        top + room
    );
    let height = row_height(cx);
    let bottom = rig
        .view
        .update(cx, |view, _| {
            view.file_line_bottom(Layout::SideBySide, height)
        })
        .expect("the picked line the menu anchors on");
    let want = first + index.lines_of(390) as usize - top;
    let down = f32::from(bottom) / f32::from(height);
    assert!(
        (down - want as f32).abs() <= 1.,
        "the menu anchors under the picked line: {down} items down, want {want}"
    );
}

#[gpui_kit::test]
fn a_pair_opened_with_wrap_on_centres_its_first_change(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    set_wrap(&rig, cx, true);
    let aimed = rig.view.update(cx, |view, _| view.file_open_at(0));
    assert!(aimed, "the file diff is open before its text lands");
    let text = Arc::new(wrapped_pair(400, &[10, 200, 390], 300));
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        DiffUpdate::Opened {
            request: 1,
            kind: PairKind::Text,
            text: Some(text),
        },
    );
    frames(&rig, cx);
    let (index, _) = index(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    let first = index.first_of(10);
    assert!(
        in_view(top, room, first),
        "the change of the opened pair is in view: item {first}, list {top}..{}",
        top + room
    );
    assert!(
        centred(top, room, first),
        "and centred, not put on top: item {first} at top {top}, room {room}"
    );
}

#[gpui_kit::test]
fn a_go_to_in_the_frame_of_a_resize_centres_the_change(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig_sized(cx, px(1400.));
    opened(&rig, cx, wrapped_pair(400, &[10, 200, 390], 300));
    set_wrap(&rig, cx, true);
    rig.press(cx, "f7");
    VisualTestContext::from_window(rig.window, cx).simulate_resize(size(px(1000.), px(800.)));
    rig.dispatch(cx, &NextDifference);
    frames(&rig, cx);
    let (index, _) = index(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    let first = index.first_of(200);
    assert!(
        in_view(top, room, first),
        "the change is in view after the resize: item {first}, list {top}..{}",
        top + room
    );
    assert!(
        centred(top, room, first),
        "and centred by the new index, not by the re-anchor: item {first} at top {top}, room {room}"
    );
}

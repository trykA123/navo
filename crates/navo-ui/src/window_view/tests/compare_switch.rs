//! Alt+V Text / Bytes (V41b): a text pair reopens in the hex body at the byte offset of its top
//! line, and back in text at the line that holds the hex body's top offset.

use std::sync::Arc;

use gpui_kit::TestAppContext;

use super::compare_hex::{frame, hex_rig, probe};
use super::compare_screen::Rig;
use super::compare_screen::body::pair;
use crate::commands::compare::{ViewBytes, ViewText};
use crate::compare_diff::{DiffUpdate, Layout, PairKind, PairText};

/// Bytes per line of the sample text, so line `n` starts at byte `16 * n`.
const LINE: u64 = 16;

/// A 400-line pair with one change on line 300 whose lines each start `LINE` bytes apart.
fn text() -> PairText {
    PairText {
        left_starts: (0..=400).map(|line| line * LINE).collect(),
        ..pair(400, &[300])
    }
}

/// Sends `Opened` for `request`.
fn open(
    cx: &mut TestAppContext,
    rig: &Rig,
    (request, kind): (u64, PairKind),
    text: Option<PairText>,
) {
    let sink = rig.diff.pair_sink();
    let opened = DiffUpdate::Opened {
        request,
        kind,
        text: text.map(Arc::new),
    };
    rig.push(cx, &sink, opened);
    frame(cx, rig);
}

/// A rig with two 8 KiB sides, the sample text opened and scrolled by Alt+Down; returns the
/// first row the text list built.
fn scrolled_text(cx: &mut TestAppContext) -> (Rig, usize) {
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Text), Some(text()));
    rig.press(cx, "alt-down");
    let (_, built, _) = rig.probe(cx, Layout::SideBySide);
    assert!(
        built[0] > 100,
        "the change at line 300 scrolled the list: {built:?}"
    );
    (rig, built[0])
}

#[gpui_kit::test]
fn compare_switch_bytes_opens_the_hex_body_at_the_top_lines_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, top) = scrolled_text(cx);
    rig.dispatch(cx, &ViewBytes);
    assert_eq!(rig.diff.views(), vec![(2, true)], "one forced bytes reload");
    open(cx, &rig, (2, PairKind::Binary), None);
    let hex = probe(cx, &rig);
    assert_eq!(hex.built.start, top, "row n holds byte 16 * n: {hex:?}");
}

#[gpui_kit::test]
fn compare_switch_text_returns_to_the_line_holding_the_hex_top_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, top) = scrolled_text(cx);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    rig.dispatch(cx, &ViewText);
    assert_eq!(rig.diff.views(), vec![(2, true), (3, false)]);
    open(cx, &rig, (3, PairKind::Text), Some(text()));
    let (current, built, _) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(built[0], top, "the same line is on top again: {built:?}");
    assert_eq!(current, None, "the text opens at the line, not at a change");
}

#[gpui_kit::test]
fn compare_switch_text_is_ignored_while_the_pair_shows_its_own_kind(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = scrolled_text(cx);
    rig.dispatch(cx, &ViewText);
    assert!(
        rig.diff.views().is_empty(),
        "a text pair has nothing to switch back"
    );
}

#[gpui_kit::test]
fn compare_switch_a_binary_pair_stays_in_bytes(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x400], vec![0x22; 0x400]);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Binary), None);
    rig.dispatch(cx, &ViewBytes);
    rig.dispatch(cx, &ViewText);
    assert!(
        rig.diff.views().is_empty(),
        "a binary pair cannot leave bytes"
    );
}

#[gpui_kit::test]
fn compare_switch_a_second_bytes_request_while_one_waits_sends_nothing(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = scrolled_text(cx);
    rig.dispatch(cx, &ViewBytes);
    rig.dispatch(cx, &ViewBytes);
    assert_eq!(rig.diff.views(), vec![(2, true)]);
}

/// The labels the View menu offers for switching, read from the open pair.
fn labels(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    rig.view.read_with(cx, |view, _| view.file_switch_labels())
}

#[gpui_kit::test]
fn compare_switch_the_menu_offers_bytes_on_text_and_text_on_bytes_only(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = scrolled_text(cx);
    assert_eq!(labels(cx, &rig), vec!["Show bytes".to_owned()]);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(labels(cx, &rig), vec!["Show text".to_owned()]);
    rig.dispatch(cx, &ViewText);
    open(cx, &rig, (3, PairKind::Text), Some(text()));
    assert_eq!(labels(cx, &rig), vec!["Show bytes".to_owned()]);
}

#[gpui_kit::test]
fn compare_switch_a_native_binary_pair_offers_no_switch(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x400], vec![0x22; 0x400]);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Binary), None);
    assert!(labels(cx, &rig).is_empty());
}

#[gpui_kit::test]
fn compare_switch_a_refresh_keeps_the_bytes_view_and_its_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, top) = scrolled_text(cx);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    assert_eq!(
        rig.diff.views(),
        vec![(2, true)],
        "a refresh is not a new choice"
    );
    assert_eq!(rig.diff.refreshes(), vec![2, 3]);
    open(cx, &rig, (3, PairKind::Binary), None);
    assert_eq!(
        probe(cx, &rig).built.start,
        top,
        "the same row stays on top"
    );
    assert_eq!(labels(cx, &rig), vec!["Show text".to_owned()]);
}

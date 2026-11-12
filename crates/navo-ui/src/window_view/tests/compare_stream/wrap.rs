//! The Stream follows Alt+Z (W8k, PC5): a card line wraps when the look says so and stays a
//! truncated one-row line when it does not, the toggle remeasures the list both ways and keeps
//! the top card, a flip made while the Stream hides behind the file diff reaches the rows on
//! return, and the View menu offers the entry while the Stream is open.

use std::sync::Arc;

use gpui_kit::{Pixels, TestAppContext, px};
use navo_core::shell_menu::ItemId;

use super::super::compare_screen::{Rig, rig};
use super::super::screen_access::screen_read;
use crate::compare_diff::{
    Card, CardRow, DiffRow, HunkView, PairKind, StreamRow, StreamUpdate, ViewedKey,
};
use crate::context_menu::ContextMenuEvent;

/// Width of one unwrapped card row, in px.
const ROW: f32 = 20.;

/// A text card with one `Changed` line whose text is `len` `w` characters, no spaces.
fn long_card(len: usize) -> Arc<Card> {
    let text = "w".repeat(len);
    let card = Card {
        kind: PairKind::Text,
        rows: vec![CardRow::Line {
            row: DiffRow::Changed(0, 0),
            hunk: 0,
            left: Some(text.clone()),
            right: Some(text),
        }],
        hunks: vec![HunkView {
            rows: 0..1,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 1,
            removed: 1,
        }],
        added: 1,
        removed: 1,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    };
    Arc::new(card)
}

/// Opens a Stream over one item and sends `card` as that item's card.
fn open_card(rig: &Rig, cx: &mut TestAppContext, card: Arc<Card>) {
    rig.open_stream(cx, 1);
    (rig.diff.stream_sink())(StreamUpdate::Card {
        generation: 1,
        id: 1,
        card,
    });
    rig.frame(cx);
}

/// Whether the open Stream's look wraps.
fn wrapped(rig: &Rig, cx: &mut TestAppContext) -> bool {
    screen_read(&rig.view, cx, |view, _| {
        view.stream_look().expect("a Stream").0.wrap
    })
}

/// The height the list gave the card's first line row; the row must be drawn to measure it.
fn line_height(rig: &Rig, cx: &mut TestAppContext) -> Pixels {
    let at = screen_read(&rig.view, cx, |view, _| {
        let (_, rows, _) = view.stream_probe().expect("a Stream");
        rows.iter()
            .position(|row| matches!(row, StreamRow::Line { .. }))
            .expect("a line row")
    });
    screen_read(&rig.view, cx, |view, _| {
        let (_, _, list) = view.stream_probe().expect("a Stream");
        list.bounds_for_item(at).expect("a drawn row").size.height
    })
}

/// The labels and checked states of the entries of the open View menu.
fn menu_entries(rig: &Rig, cx: &mut TestAppContext) -> Vec<(String, bool)> {
    let menu =
        screen_read(&rig.view, cx, |view, _| view.compare_menu_view()).expect("an open View menu");
    menu.read_with(cx, |menu, _| {
        menu.menu()
            .items
            .iter()
            .map(|item| (item.text.clone(), item.checked))
            .collect()
    })
}

/// Whether the open View menu offers "Wrap lines", and whether it shows it checked.
fn wrap_entry(rig: &Rig, cx: &mut TestAppContext) -> Option<bool> {
    menu_entries(rig, cx)
        .iter()
        .find(|(text, ..)| text == "Wrap lines")
        .map(|(.., checked)| *checked)
}

#[gpui_kit::test]
fn compare_stream_wrap_on_a_500_character_line_draws_taller_than_one_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_card(&rig, cx, long_card(500));
    assert_eq!(line_height(&rig, cx), px(ROW), "wrap off is one row tall");
    rig.press(cx, "alt-z");
    assert!(wrapped(&rig, cx), "Alt+Z flips wrap in the Stream");
    let tall = line_height(&rig, cx);
    assert!(
        tall > px(ROW),
        "a card line of 500 characters without spaces draws taller than one row: {tall:?}"
    );
}

#[gpui_kit::test]
fn compare_stream_wrap_off_keeps_the_long_line_one_row_tall(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_card(&rig, cx, long_card(500));
    rig.press(cx, "alt-z");
    assert!(wrapped(&rig, cx), "Alt+Z flips wrap on");
    assert!(
        line_height(&rig, cx) > px(ROW),
        "the tall row is the wrapped one"
    );
    rig.press(cx, "alt-z");
    assert!(!wrapped(&rig, cx), "Alt+Z again flips wrap off");
    assert_eq!(
        line_height(&rig, cx),
        px(ROW),
        "wrap off keeps the same line at the one-row height, truncated"
    );
}

/// Closes the open View menu choosing the entry that runs `compare::ToggleWrap`, as a click
/// would; the menu's own event path then dispatches it.
fn choose_wrap(rig: &Rig, cx: &mut TestAppContext) {
    let menu =
        screen_read(&rig.view, cx, |view, _| view.compare_menu_view()).expect("an open View menu");
    menu.update(cx, |_, cx| {
        cx.emit(ContextMenuEvent::Chosen(ItemId::Action(
            "compare::ToggleWrap",
        )));
    });
    cx.run_until_parked();
    rig.frame(cx);
}

#[gpui_kit::test]
fn compare_stream_the_view_menu_offers_the_wrap_entry_with_a_check(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 1);
    rig.press(cx, "alt-v");
    assert_eq!(
        wrap_entry(&rig, cx),
        Some(false),
        "the Stream's View menu shows Wrap lines, unchecked"
    );
    choose_wrap(&rig, cx);
    assert!(
        wrapped(&rig, cx),
        "choosing the entry flips wrap on the Stream"
    );
    rig.press(cx, "alt-v");
    assert_eq!(
        wrap_entry(&rig, cx),
        Some(true),
        "and the entry shows checked while wrap is on"
    );
}

#[gpui_kit::test]
fn compare_stream_a_wrap_flip_made_behind_the_file_diff_wraps_the_rows_on_return(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_card(&rig, cx, long_card(500));
    assert_eq!(
        line_height(&rig, cx),
        px(ROW),
        "the card line starts on one row"
    );
    rig.press(cx, "enter");
    rig.press(cx, "alt-z");
    rig.press(cx, "escape");
    rig.frame(cx);
    assert!(
        wrapped(&rig, cx),
        "the file diff's wrap came back to the Stream (D39.8)"
    );
    let tall = line_height(&rig, cx);
    assert!(
        tall > px(ROW),
        "the hidden flip remeasured the rows: the wrapped card line is taller than one row: {tall:?}"
    );
}

#[gpui_kit::test]
fn compare_stream_a_wrap_flip_keeps_the_top_card(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 60);
    // A long card of item 2 sits above the view: flipping wrap grows its rows and moves
    // everything below it in px, but not the anchored top card.
    (rig.diff.stream_sink())(StreamUpdate::Card {
        generation: 1,
        id: 2,
        card: long_card(500),
    });
    rig.frame(cx);
    rig.scroll_to_item(cx, 30);
    let (before, row) = rig.top(cx);
    assert_eq!(
        row,
        StreamRow::Header(30),
        "item 31's header is the top row"
    );
    rig.press(cx, "alt-z");
    assert!(wrapped(&rig, cx), "Alt+Z flipped wrap on the Stream");
    assert_eq!(
        rig.top(cx),
        (before, StreamRow::Header(30)),
        "the scroll-top card is the same card after the flip"
    );
}

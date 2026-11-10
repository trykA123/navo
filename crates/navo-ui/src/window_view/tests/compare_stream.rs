//! The Stream screen (V19): J walks hunks across cards, a card arriving above the view keeps
//! the top row, V counts and moves on, and the app's focus carries the ids in view.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{Action, AppContext as _, ListOffset, TestAppContext, px};

use super::compare_screen::{Rig, rig, rig_sized};
use super::settle;
use crate::browser::RowStatus;
use crate::commands::compare::ToggleViewed;
use crate::compare_diff::{
    Card, CardRow, DiffRow, HunkView, PairKind, PairRequest, Stop, StreamItem, StreamList,
    StreamRow, StreamUpdate, ViewedKey,
};

mod entry;
mod search;
mod vr1;

/// `count` differing items with ids 1..=count, and their pair requests.
pub(super) fn items(count: u64) -> (StreamList, Vec<PairRequest>) {
    let items: Vec<StreamItem> = (1..=count)
        .map(|id| StreamItem {
            id,
            path: format!("f{id:03}.txt"),
            status: RowStatus::Different,
            settled: true,
        })
        .collect();
    let requests = items
        .iter()
        .map(|item| PairRequest {
            id: item.id,
            path: item.path.clone(),
            left: None,
            right: None,
        })
        .collect();
    (StreamList { items, rules: 0 }, requests)
}

/// A text card with `hunks` hunks, each one context line and one changed line, gaps between.
fn card(hunks: u32) -> Arc<Card> {
    let line = |row: DiffRow, hunk: u32| CardRow::Line {
        row,
        hunk,
        left: Some("left".into()),
        right: Some("right".into()),
    };
    let mut rows = Vec::new();
    for hunk in 0..hunks {
        if hunk > 0 {
            rows.push(CardRow::Gap { lines: 5 });
        }
        let at = hunk * 10;
        rows.push(line(DiffRow::Equal(at, at), hunk));
        rows.push(line(DiffRow::Changed(at + 1, at + 1), hunk));
    }
    let views = (0..hunks)
        .map(|hunk| HunkView {
            rows: hunk * 10 + 1..hunk * 10 + 2,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 1,
            removed: 1,
        })
        .collect();
    Arc::new(Card {
        kind: PairKind::Text,
        rows,
        hunks: views,
        added: hunks,
        removed: hunks,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    })
}

impl Rig {
    /// Opens a Stream over `count` items and draws it.
    pub(super) fn open_stream(&self, cx: &mut TestAppContext, count: u64) {
        let items = items(count);
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| view.open_stream(items, window, cx))
        })
        .expect("window");
        self.frame(cx);
    }

    /// Settles the window and draws a frame.
    pub(super) fn frame(&self, cx: &mut TestAppContext) {
        settle(cx, &self.view);
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }

    /// Whether the outline drawer is open.
    fn outline_open(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| {
            view.stream_probe().expect("a Stream").0.outline_open()
        })
    }

    /// Sends the card of item `id` as the app would.
    fn send_card(&self, cx: &mut TestAppContext, id: u64, hunks: u32) {
        let sink = self.diff.stream_sink();
        sink(StreamUpdate::Card {
            generation: 1,
            id,
            card: card(hunks),
        });
        self.frame(cx);
    }

    /// Runs a boxed `action` on the focused element, then draws a frame.
    fn send(&self, cx: &mut TestAppContext, action: Box<dyn Action>) {
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_action(action, cx);
        })
        .expect("window");
        self.frame(cx);
    }

    fn cursor(&self, cx: &mut TestAppContext) -> Stop {
        self.view.read_with(cx, |view, _| {
            view.stream_probe().expect("a Stream").0.cursor()
        })
    }

    /// The list's top row index and that row.
    fn top(&self, cx: &mut TestAppContext) -> (usize, StreamRow) {
        self.view.read_with(cx, |view, _| {
            let (_, rows, list) = view.stream_probe().expect("a Stream");
            let top = list.logical_scroll_top().item_ix;
            (top, rows[top])
        })
    }

    /// Scrolls the list so the header of item `item` is the top row.
    fn scroll_to_item(&self, cx: &mut TestAppContext, item: usize) {
        self.view.read_with(cx, |view, _| {
            let (_, rows, list) = view.stream_probe().expect("a Stream");
            let at = rows.iter().position(|row| *row == StreamRow::Header(item));
            list.scroll_to(ListOffset {
                item_ix: at.expect("a header"),
                offset_in_item: px(0.),
            });
        });
        self.frame(cx);
    }
}

/// A stop at `item`, hunk `hunk`.
fn stop(item: usize, hunk: u32) -> Stop {
    Stop {
        item,
        hunk: Some(hunk),
    }
}

#[gpui_kit::test]
fn compare_stream_j_walks_hunks_across_two_cards(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_card(cx, 1, 2);
    rig.send_card(cx, 2, 2);
    assert_eq!(rig.cursor(cx), stop(0, 0));
    let mut walked = Vec::new();
    for _ in 0..4 {
        rig.press(cx, "j");
        walked.push(rig.cursor(cx));
    }
    let expected = [stop(0, 1), stop(1, 0), stop(1, 1), stop(1, 1)];
    assert_eq!(walked, expected, "J stops at the last change");
    rig.press(cx, "k");
    rig.press(cx, "k");
    assert_eq!(
        rig.cursor(cx),
        stop(0, 1),
        "K walks back into the first card"
    );
}

#[gpui_kit::test]
fn compare_stream_a_card_arriving_above_the_view_keeps_the_top_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 60);
    rig.scroll_to_item(cx, 30);
    let (top, row) = rig.top(cx);
    assert_eq!(row, StreamRow::Header(30));
    rig.send_card(cx, 6, 3);
    // Three hunks give 2 + 1 + 2 + 1 + 2 rows in place of one placeholder.
    assert_eq!(rig.top(cx), (top + 7, StreamRow::Header(30)));
    rig.send_card(cx, 45, 3);
    assert_eq!(
        rig.top(cx),
        (top + 7, StreamRow::Header(30)),
        "below the view"
    );
}

#[gpui_kit::test]
fn compare_stream_v_counts_and_moves_on(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_card(cx, 1, 2);
    rig.send_card(cx, 2, 1);
    rig.send(cx, Box::new(ToggleViewed));
    assert_eq!(rig.diff.viewed(), vec![(1, true)]);
    assert_eq!(rig.cursor(cx), stop(1, 0), "V moves to the next file");
    let (counter, folded) = rig.view.read_with(cx, |view, _| {
        let (model, rows, _) = view.stream_probe().expect("a Stream");
        let folded = !rows
            .iter()
            .any(|row| matches!(row, StreamRow::Line { item: 0, .. }));
        (model.counter(), folded)
    });
    assert_eq!(counter, (1, 2));
    assert!(folded, "a viewed card shows its header only");
}

#[gpui_kit::test]
fn compare_stream_focus_carries_visible_ids(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 260);
    let first = rig.diff.last_focus().expect("a focus on open");
    assert_eq!(
        first,
        (1..=11).collect::<Vec<u64>>(),
        "the top item and 10 after it"
    );
    rig.send_card(cx, 1, 2);
    rig.scroll_to_item(cx, 250);
    rig.view.update(cx, |view, cx| view.stream_scrolled(cx));
    cx.executor().advance_clock(Duration::from_millis(200));
    rig.frame(cx);
    let ids = rig.diff.last_focus().expect("a focus after the scroll");
    assert!(ids.contains(&251) && ids.contains(&241), "{ids:?}");
    assert!(
        !ids.contains(&240),
        "10 items before the view, no more: {ids:?}"
    );
    let kept = rig.view.read_with(cx, |view, _| {
        view.stream_probe().expect("a Stream").0.card(1).is_some()
    });
    assert!(!kept, "a card 250 items away is dropped");
}

#[gpui_kit::test]
fn compare_stream_esc_leaves_to_the_panes(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.press(cx, "escape");
    let open = rig
        .view
        .read_with(cx, |view, _| view.compare.screen.is_some());
    assert!(!open, "Esc closes the Stream");
}

#[gpui_kit::test]
fn compare_stream_outline_opens_closed_and_ctrl_b_toggles_it(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 3);
    assert!(!rig.outline_open(cx), "calm by default");
    rig.press(cx, "ctrl-b");
    assert!(rig.outline_open(cx));
    rig.press(cx, "ctrl-b");
    assert!(!rig.outline_open(cx));
    rig.press(cx, "ctrl-b");
    rig.press(cx, "escape");
    assert!(!rig.outline_open(cx), "Esc closes the outline first");
    let open = rig
        .view
        .read_with(cx, |view, _| view.compare.screen.is_some());
    assert!(open, "the Stream stays");
}

#[gpui_kit::test]
fn compare_stream_outline_enter_jumps_to_the_picked_file(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 40);
    rig.press(cx, "ctrl-b");
    for key in ["down", "down", "down", "enter"] {
        rig.press(cx, key);
    }
    assert_eq!(rig.cursor(cx).item, 3, "Enter jumps to the third file down");
    assert_eq!(
        rig.top(cx).1,
        StreamRow::Header(3),
        "its card is the top row"
    );
    assert!(rig.outline_open(cx), "a wide window keeps the drawer");
}

#[gpui_kit::test]
fn compare_stream_outline_closes_after_a_jump_in_a_narrow_window(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig_sized(cx, px(700.));
    rig.open_stream(cx, 40);
    rig.press(cx, "ctrl-b");
    rig.press(cx, "down");
    rig.press(cx, "enter");
    assert_eq!(rig.cursor(cx).item, 1);
    assert!(!rig.outline_open(cx), "below 900 px the jump closes it");
}

#[gpui_kit::test]
fn compare_stream_outline_current_follows_the_scroll(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 60);
    let in_view =
        |cx: &mut TestAppContext| rig.view.read_with(cx, |view, _| view.outline_in_view());
    assert_eq!(in_view(cx), Some(0));
    rig.scroll_to_item(cx, 30);
    assert_eq!(in_view(cx), Some(30));
    rig.scroll_to_item(cx, 12);
    assert_eq!(in_view(cx), Some(12));
}

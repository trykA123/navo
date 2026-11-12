//! The Stream's search (V29): `/` opens the field, typing narrows after a pause, J and K walk the
//! matches, Esc clears and closes, and a new item list asks the app again.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::screen_access::{screen_read, screen_update};
use super::{Rig, items, rig, stop};
use crate::compare_diff::{StreamRow, StreamUpdate};

impl Rig {
    /// Types `text` into the focused element, as the keyboard would.
    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        self.frame(cx);
    }

    /// Lets `ms` milliseconds pass and draws a frame.
    fn wait(&self, cx: &mut TestAppContext, ms: u64) {
        cx.executor().advance_clock(Duration::from_millis(ms));
        self.frame(cx);
    }

    /// The (open, field text, last query tag) of the search.
    fn field(&self, cx: &mut TestAppContext) -> (bool, String, u64) {
        screen_read(&self.view, cx, |view, cx| view.search_probe(cx)).expect("a field")
    }

    /// The queries the fake app received.
    fn searches(&self) -> Vec<(u64, String)> {
        self.diff.calls.searches.lock().unwrap().clone()
    }

    /// Answers the last query as the app would.
    fn answer(
        &self,
        cx: &mut TestAppContext,
        (query, done): (u64, bool),
        hits: Vec<(u64, Vec<u32>)>,
    ) {
        let sink = self.diff.stream_sink();
        sink(StreamUpdate::Hits {
            generation: 1,
            query,
            hits,
            searched: 3,
            total: 3,
            done,
        });
        self.frame(cx);
    }

    /// Opens a Stream of three files with three hunks each, and types `text` in the field.
    fn open_search(&self, cx: &mut TestAppContext, text: &str) {
        self.open_stream(cx, 3);
        for id in 1..=3 {
            self.send_card(cx, id, 3);
        }
        self.press(cx, "/");
        self.type_text(cx, text);
        self.wait(cx, 150);
    }

    /// How many rows of each kind the list holds: (lines, others).
    fn row_kinds(&self, cx: &mut TestAppContext) -> (usize, usize) {
        screen_read(&self.view, cx, |view, _| {
            let (_, rows, _) = view.stream_probe().expect("a Stream");
            let lines = rows
                .iter()
                .filter(|row| matches!(row, StreamRow::Line { .. }))
                .count();
            let others = rows
                .iter()
                .filter(|row| matches!(row, StreamRow::Others { .. }))
                .count();
            (lines, others)
        })
    }
}

#[gpui_kit::test]
fn compare_search_slash_opens_the_field_and_a_pause_sends_one_query(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 3);
    rig.press(cx, "/");
    assert!(rig.field(cx).0, "the field opens");
    rig.type_text(cx, "a");
    rig.wait(cx, 100);
    rig.type_text(cx, "b");
    rig.wait(cx, 100);
    assert!(rig.searches().is_empty(), "typing restarts the 150 ms wait");
    rig.wait(cx, 60);
    assert_eq!(rig.searches(), vec![(1, "ab".to_owned())]);
}

#[gpui_kit::test]
fn compare_search_letters_type_into_the_field_and_do_not_move_the_cursor(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_card(cx, 1, 2);
    rig.send_card(cx, 2, 2);
    let before = rig.cursor(cx);
    rig.press(cx, "/");
    for key in ["j", "k", "v", "o"] {
        rig.press(cx, key);
    }
    assert_eq!(rig.field(cx).1, "jkvo");
    assert_eq!(rig.cursor(cx), before, "J and K typed, not run");
}

#[gpui_kit::test]
fn compare_search_hits_narrow_the_cards_and_fold_the_other_changes(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, false), vec![(1, vec![0]), (3, vec![1, 2])]);
    let counts = screen_read(&rig.view, cx, |view, _| {
        let model = view.stream_probe().expect("a Stream").0;
        (
            model.visible(),
            model.match_counts(),
            model.search_progress(),
        )
    });
    assert_eq!(counts, (vec![0, 2], (3, 2), Some((3, 3))));
    // Item 0 keeps hunk 0 and folds two; item 2 folds hunk 0 and keeps two.
    assert_eq!(rig.row_kinds(cx), (6, 2));
    rig.answer(cx, (1, true), vec![(1, vec![0]), (3, vec![1, 2])]);
    let progress = screen_read(&rig.view, cx, |view, _| {
        view.stream_probe().expect("a Stream").0.search_progress()
    });
    assert_eq!(progress, None, "done ends the progress line");
}

#[gpui_kit::test]
fn compare_search_j_and_k_walk_the_matches_only(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(1, vec![1]), (3, vec![0, 2])]);
    rig.press(cx, "enter");
    assert_eq!(
        rig.cursor(cx),
        stop(0, 1),
        "Enter hands focus to the first match"
    );
    let mut walked = Vec::new();
    for _ in 0..3 {
        rig.press(cx, "j");
        walked.push(rig.cursor(cx));
    }
    assert_eq!(walked, [stop(2, 0), stop(2, 2), stop(2, 2)]);
}

#[gpui_kit::test]
fn compare_search_esc_clears_closes_and_stops_the_app(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(2, vec![0])]);
    rig.press(cx, "escape");
    assert!(!rig.field(cx).0, "the field closes");
    assert_eq!(rig.searches().last(), Some(&(2, String::new())));
    let (searching, shown) = screen_read(&rig.view, cx, |view, _| {
        let model = view.stream_probe().expect("a Stream").0;
        (model.searching(), model.visible().len())
    });
    assert!(!searching && shown == 3, "every card is back");
    let open = screen_read(&rig.view, cx, |view, _| view.compare.screen.is_some());
    assert!(open, "the first Esc only closes the search");
    rig.press(cx, "j");
    assert_eq!(
        rig.cursor(cx),
        stop(1, 1),
        "the cursor stays; focus is back on the cards"
    );
}

#[gpui_kit::test]
fn compare_search_emptying_the_field_clears_without_a_final_hit(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(2, vec![0])]);
    rig.press(cx, "backspace");
    rig.press(cx, "backspace");
    rig.wait(cx, 150);
    assert_eq!(rig.searches().last(), Some(&(2, String::new())));
    let searching = screen_read(&rig.view, cx, |view, _| {
        view.stream_probe().expect("a Stream").0.searching()
    });
    assert!(!searching);
    assert!(rig.field(cx).0, "an empty field stays open");
}

#[gpui_kit::test]
fn compare_search_a_new_item_list_asks_the_app_again(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(2, vec![])]);
    let replaced = screen_update(&rig.view, cx, |view, cx| {
        view.stream_replace(items(4), true, cx)
    });
    assert!(replaced);
    rig.frame(cx);
    assert_eq!(rig.searches().last(), Some(&(2, "ab".to_owned())));
    let visible = screen_read(&rig.view, cx, |view, _| {
        view.stream_probe().expect("a Stream").0.visible()
    });
    assert_eq!(visible, [1], "the hits stay until the new ones come");
}

#[gpui_kit::test]
fn compare_search_same_generation_updates_rerun_at_most_once_a_second(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(2, vec![])]);
    for count in [4, 5, 6] {
        screen_update(&rig.view, cx, |view, cx| {
            view.stream_replace(items(count), false, cx)
        });
        rig.frame(cx);
    }
    let asked = rig.searches();
    assert_eq!(asked.len(), 2, "one at once, the rest wait: {asked:?}");
    rig.wait(cx, 1100);
    let asked = rig.searches();
    assert_eq!(asked.len(), 3, "one trailing rerun for the last batch");
    assert_eq!(asked.last(), Some(&(3, "ab".to_owned())));
}

#[gpui_kit::test]
fn compare_search_a_fresh_generation_reruns_at_once(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_search(cx, "ab");
    rig.answer(cx, (1, true), vec![(2, vec![])]);
    for (count, fresh) in [(4, false), (5, true)] {
        screen_update(&rig.view, cx, |view, cx| {
            view.stream_replace(items(count), fresh, cx)
        });
        rig.frame(cx);
    }
    assert_eq!(rig.searches().len(), 3, "the fresh update did not wait");
}

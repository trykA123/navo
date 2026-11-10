//! VR1 fixes of the Stream: row batches keep the generation and the cards (at most one rebuild
//! per 500 ms, at once when the compare finishes), walking back from a pane diff, far cards
//! dropped on arrival, and Alt+V and Ctrl+I in the Stream.

use std::time::Duration;

use gpui_kit::TestAppContext;

use super::super::compare_screen::rig;
use super::entry::row;
use super::stop;
use crate::browser::{CompareUpdate, RowStatus};
use crate::compare_diff::{Half, Layout, StreamRow};
use crate::window_view::compare_screen::look::Preset;
use crate::window_view::compare_screen::menu::ShowUnified;
use crate::window_view::tests::settle;
use crate::window_view::viewer::ViewSimple;

#[gpui_kit::test]
fn compare_stream_row_batches_keep_the_generation_and_the_cards(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.send_card(cx, 1, 2);
    rig.send_rows(cx, vec![row(10, "c.txt", RowStatus::Different)]);
    assert_eq!(
        rig.item_paths(cx).len(),
        3,
        "the first batch rebuilds at once"
    );
    for (id, path) in [(11, "d.txt"), (12, "e.txt")] {
        rig.send_rows(cx, vec![row(id, path, RowStatus::Different)]);
    }
    assert_eq!(rig.item_paths(cx).len(), 3, "batches within 500 ms wait");
    cx.executor().advance_clock(Duration::from_millis(500));
    rig.frame(cx);
    assert_eq!(rig.item_paths(cx).len(), 5, "the wait ends in one rebuild");
    rig.send_rows(cx, vec![row(13, "f.txt", RowStatus::Different)]);
    assert_eq!(rig.item_paths(cx).len(), 5);
    (rig.compare)(CompareUpdate::Finished { generation: 1 });
    settle(cx, &rig.view);
    assert_eq!(
        rig.item_paths(cx).len(),
        6,
        "a finished compare rebuilds at once"
    );
    let items = rig.diff.calls.items.lock().unwrap().clone();
    assert_eq!(
        items,
        vec![(1, 2), (1, 3), (1, 5), (1, 6)],
        "one generation"
    );
    let kept = rig.view.read_with(cx, |view, _| {
        view.stream_probe().expect("a Stream").0.card(1).is_some()
    });
    assert!(kept, "the built card stays");
}

#[gpui_kit::test]
fn compare_stream_previous_file_from_a_pane_diff_skips_rows_that_are_no_item(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.send_rows(
        cx,
        vec![
            row(3, "c.txt", RowStatus::Same),
            row(4, "d.txt", RowStatus::OnlyLeft),
        ],
    );
    rig.press(cx, "enter");
    for _ in 0..2 {
        rig.press(cx, "ctrl-alt-down");
    }
    assert_eq!(rig.shown(cx), Some((4, "d.txt".into())));
    rig.press(cx, "ctrl-alt-up");
    assert_eq!(rig.shown(cx), Some((2, "b.txt".into())), "c.txt is Same");
    rig.press(cx, "ctrl-alt-up");
    rig.press(cx, "ctrl-alt-up");
    assert_eq!(
        rig.shown(cx),
        Some((1, "a.txt".into())),
        "stops at the first"
    );
}

#[gpui_kit::test]
fn compare_stream_a_card_far_from_the_view_is_dropped_on_arrival(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 500);
    rig.send_card(cx, 450, 1);
    rig.send_card(cx, 2, 1);
    let (far, near) = rig.view.read_with(cx, |view, _| {
        let model = view.stream_probe().expect("a Stream").0;
        (model.card(450).is_some(), model.card(2).is_some())
    });
    assert!(!far, "449 items below the view");
    assert!(near, "a card in view stays");
}

#[gpui_kit::test]
fn compare_stream_alt_v_sets_the_layout_and_the_look_a_file_diff_starts_with(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.send_card(cx, 1, 1);
    rig.send(cx, Box::new(ShowUnified));
    let unified = rig.view.read_with(cx, |view, _| {
        let rows = view.stream_probe().expect("a Stream").1;
        rows.iter().any(|row| {
            matches!(
                row,
                StreamRow::Line {
                    half: Half::Left,
                    ..
                }
            )
        })
    });
    assert!(unified, "the View menu's layout applies to the cards");
    rig.send(cx, Box::new(ViewSimple));
    rig.press(cx, "enter");
    let (look, _, _) = rig
        .view
        .read_with(cx, |v, _| v.file_look())
        .expect("a file diff");
    assert_eq!(
        look.preset,
        Preset::Simple,
        "the file diff starts with the Stream's look"
    );
    assert_eq!(look.layout, Some(Layout::Unified));
    rig.press(cx, "escape");
    rig.press(cx, "alt-v");
    let menu = rig
        .view
        .read_with(cx, |v, _| v.compare_menu_view().is_some());
    assert!(menu, "Alt+V opens the View menu over the Stream");
}

#[gpui_kit::test]
fn compare_stream_ctrl_i_shows_the_current_file_and_esc_closes_it_first(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.send_card(cx, 1, 1);
    rig.press(cx, "ctrl-i");
    let details = rig
        .view
        .read_with(cx, |v, _| v.stream_look())
        .and_then(|l| l.1);
    let details = details.expect("Ctrl+I opens the details drawer");
    assert_eq!(details[0], "a.txt");
    assert_eq!(details.len(), 3, "path, status, changes: {details:?}");
    rig.press(cx, "escape");
    let (_, details) = rig
        .view
        .read_with(cx, |v, _| v.stream_look())
        .expect("a Stream");
    assert!(
        details.is_none(),
        "Esc closes the drawer and keeps the Stream"
    );
    assert_eq!(rig.cursor(cx), stop(0, 0));
}

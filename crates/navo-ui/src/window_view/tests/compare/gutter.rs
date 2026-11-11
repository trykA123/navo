//! The folder compare's centre gutter (D33): the pane that shows the compare's left side draws the
//! label column at its trailing edge, the other pane draws none, and no row draws the old badge.

use std::sync::Arc;

use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext};
use navo_core::Command;
use navo_l10n::tr;

use super::{Rig, frames, rig};
use crate::browser::{
    CompareKind, CompareRow, CompareSide, CompareUpdate, RowStatus, RowView, pane_rows,
};
use crate::compare_list::GutterText;
use crate::window_view::tests::settle;

/// The mark glyphs the badge column used to draw.
const MARKS: [&str; 8] = ["⊃", "=", "≠", "←", "→", "…", "–", "!"];

/// The old badge column and the gutter share a row, so a test compares labels exactly.
fn drawn_labels(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, _| {
        window
            .elements()
            .iter()
            .filter_map(|e| e.label().map(str::to_owned))
            .collect()
    })
    .expect("window")
}

/// The gutter cells drawn for row `index`, window-wide: each one's bounds.
fn gutter_cells(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    index: usize,
) -> Vec<Bounds<Pixels>> {
    cx.update_window(window, |_, window, _| {
        window
            .find_all(("gutter", index))
            .iter()
            .map(|cell| cell.bounds())
            .collect()
    })
    .expect("window")
}

/// The bounds of the compare list that shows the side named `label`.
fn side_bounds(cx: &mut TestAppContext, window: AnyWindowHandle, label: &str) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| window.find_by_label(label).bounds())
        .expect("window")
}

/// Whether a cell lies inside its pane's list, so it belongs to that pane.
fn inside(cell: Bounds<Pixels>, list: Bounds<Pixels>) -> bool {
    cell.origin.x >= list.origin.x
        && cell.origin.x + cell.size.width <= list.origin.x + list.size.width
}

impl Rig {
    /// Sends rows with these statuses; `RowStatus::OnlyRight` has no file on the left, so the left
    /// pane draws a gap row there.
    fn send_statuses(&self, cx: &mut TestAppContext, rows: &[(&str, RowStatus)]) {
        let file = Some(CompareSide {
            kind: CompareKind::File,
            size: 3,
            modified_ms: None,
        });
        let sent: Vec<CompareRow> = (1u64..)
            .zip(rows)
            .map(|(id, (name, status))| {
                let sides = match status {
                    RowStatus::OnlyRight => (None, file),
                    _ => (file, file),
                };
                CompareRow::new(id, (*name).to_owned(), *status, sides)
            })
            .collect();
        let sink = self
            .sink
            .lock()
            .unwrap()
            .clone()
            .expect("a started compare");
        sink(CompareUpdate::Rows {
            generation: 1,
            rows: sent,
            progress: Vec::new(),
        });
        settle(cx, &self.view);
    }

    /// Swaps the two panes' slots, the way a shift-drag or the layout switcher does.
    fn swap_panes(&self, cx: &mut TestAppContext) {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.apply(&Command::LayoutSwap { a: 1, b: 2 }, window, cx);
            });
        })
        .expect("window");
        settle(cx, &self.view);
    }

    /// The rows both panes draw now, as the compare view builds them: side 0 first.
    fn side_rows(&self, cx: &mut TestAppContext) -> [Arc<[RowView]>; 2] {
        self.view.read_with(cx, |view, _| {
            let mode = view.browser.compare_mode().expect("a started compare");
            [pane_rows(mode, 0), pane_rows(mode, 1)]
        })
    }
}

#[gpui_kit::test]
fn the_left_side_pane_draws_the_gutter_and_the_right_side_draws_none(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_statuses(
        cx,
        &[
            ("a.txt", RowStatus::Different),
            ("z.txt", RowStatus::OnlyRight),
        ],
    );
    frames(cx, rig.window);
    let left = side_bounds(cx, rig.window, &tr("compare-list-left"));
    let right = side_bounds(cx, rig.window, &tr("compare-list-right"));
    let changed = gutter_cells(cx, rig.window, 0);
    assert_eq!(
        changed.len(),
        1,
        "only the pane that shows the left side draws a gutter cell"
    );
    assert!(
        inside(changed[0], left) && !inside(changed[0], right),
        "the changed pair's gutter cell sits in the left side's pane: {:?}",
        changed[0]
    );
    assert_eq!(
        rig.side_rows(cx)[0][0]
            .gutter
            .as_ref()
            .map(|label| label.text.words()),
        Some(tr("compare-gutter-changed")),
        "that cell says the pair is changed"
    );
    let gap = gutter_cells(cx, rig.window, 1);
    assert_eq!(
        gap.len(),
        1,
        "a gap row in the left pane still gets its gutter cell"
    );
    assert!(
        inside(gap[0], left),
        "the gap row's gutter is in the same column"
    );
}

#[gpui_kit::test]
fn a_pane_swap_keeps_the_gutter_on_the_pane_that_shows_the_left_side(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_statuses(cx, &[("a.txt", RowStatus::Different)]);
    frames(cx, rig.window);
    let before = side_bounds(cx, rig.window, &tr("compare-list-left"));
    rig.swap_panes(cx);
    frames(cx, rig.window);
    let after = side_bounds(cx, rig.window, &tr("compare-list-left"));
    assert!(
        after.origin.x > before.origin.x,
        "the swap moved the left side to the other half: {before:?} -> {after:?}"
    );
    let cells = gutter_cells(cx, rig.window, 0);
    assert_eq!(
        cells.len(),
        1,
        "one gutter cell window-wide, still on the left side"
    );
    assert!(
        inside(cells[0], after),
        "the gutter followed the pane that shows the left side: {:?}",
        cells[0]
    );
}

#[gpui_kit::test]
fn no_compare_row_draws_the_badge_column_anymore(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_statuses(
        cx,
        &[
            ("same.txt", RowStatus::Same),
            ("a.txt", RowStatus::Different),
            ("only.txt", RowStatus::OnlyLeft),
            ("right.txt", RowStatus::OnlyRight),
            ("type.txt", RowStatus::TypeConflict),
            ("check.txt", RowStatus::Checking),
            ("skip.txt", RowStatus::NotChecked("compare-reason-stopped")),
        ],
    );
    frames(cx, rig.window);
    let drawn = drawn_labels(cx, rig.window);
    for mark in MARKS {
        assert!(
            !drawn.iter().any(|label| label == mark),
            "the badge column is gone, yet {mark} is drawn in {drawn:?}"
        );
    }
}

#[gpui_kit::test]
fn the_left_side_row_carries_the_gutter_label_and_the_right_side_row_does_not(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_statuses(cx, &[("a.txt", RowStatus::Different)]);
    let [left, right] = rig.side_rows(cx);
    assert_eq!(
        left.first()
            .and_then(|row| row.gutter.as_ref())
            .map(|label| label.text),
        Some(GutterText::Changed),
        "the pane that shows the left side carries the pair's gutter label"
    );
    assert!(
        right.first().and_then(|row| row.gutter.as_ref()).is_none(),
        "the pane that shows the right side carries no gutter label"
    );
}

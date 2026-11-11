//! Into the Stream and back (V20): Alt+F opens it from the panes, Enter and Esc round trip keep
//! the card, Ctrl+Alt+↓ walks files from the diff, leaving reveals the file, rows refresh items.

use std::sync::Arc;

use gpui_kit::TestAppContext;

use super::super::compare_screen::{Rig, rig};
use super::super::settle;
use super::stop;
use crate::browser::{CompareKind, CompareRow, CompareSide, CompareUpdate, RowStatus};
use crate::compare_diff::{
    DiffRow, DiffUpdate, HunkView, PairFacts, PairKind, PairText, ViewedKey,
};

/// A text pair of six lines with a one-line change at lines 1 and 4 (two hunks).
fn two_hunks() -> Arc<PairText> {
    let rows = (0..6)
        .map(|at| match at % 3 {
            1 => DiffRow::Changed(at, at),
            _ => DiffRow::Equal(at, at),
        })
        .collect();
    let hunk = |at: u32| HunkView {
        rows: at..at + 1,
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: None,
        added: 1,
        removed: 1,
    };
    Arc::new(PairText {
        left_starts: Vec::new(),
        left: vec![String::new(); 6],
        right: vec![String::new(); 6],
        rows,
        hunks: vec![hunk(1), hunk(4)],
        facts: PairFacts {
            left_encoding: "cmd-viewer-encoding",
            right_encoding: "cmd-viewer-encoding",
            left_endings: "",
            right_endings: "",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    })
}

/// A compare row at `path` with `status`; a path without a dot is a folder pair.
pub(super) fn row(id: u64, path: &str, status: RowStatus) -> CompareRow {
    let kind = match path.contains('.') {
        true => CompareKind::File,
        false => CompareKind::Folder,
    };
    let side = Some(CompareSide {
        kind,
        size: 3,
        modified_ms: None,
    });
    CompareRow {
        id,
        path: path.to_owned(),
        left: side,
        right: side,
        status,
        contains_differences: false,
    }
}

impl Rig {
    /// Sends `rows` as more rows of the running compare and settles.
    pub(super) fn send_rows(&self, cx: &mut TestAppContext, rows: Vec<CompareRow>) {
        (self.compare)(CompareUpdate::Rows {
            generation: 1,
            rows,
            progress: Vec::new(),
        });
        settle(cx, &self.view);
    }

    /// The paths of the Stream's items.
    pub(super) fn item_paths(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let (model, _, _) = view.stream_probe().expect("a Stream");
            model.items().iter().map(|item| item.path.clone()).collect()
        })
    }

    /// The id and path of the open file diff's pair.
    pub(super) fn shown(&self, cx: &mut TestAppContext) -> Option<(u64, String)> {
        self.view.read_with(cx, |view, _| {
            view.file_screen()
                .map(|(request, _)| (request.id, request.path.clone()))
        })
    }

    /// Delivers a two-hunk text to the last opened pair; returns the file view's current change.
    fn open_two_hunks(&self, cx: &mut TestAppContext) -> Option<usize> {
        let request = self
            .view
            .read_with(cx, |view, _| {
                view.file_screen().map(|(request, _)| request.id)
            })
            .unwrap_or(1);
        let opened = DiffUpdate::Opened {
            request,
            kind: PairKind::Text,
            text: Some(two_hunks()),
        };
        (self.diff.pair_sink())(opened);
        self.frame(cx);
        self.view.update(cx, |view, _| {
            view.file_probe(crate::compare_diff::Layout::SideBySide)
                .and_then(|(current, _, _)| current)
        })
    }

    /// The compare's shown folder and selected path.
    fn compare_place(&self, cx: &mut TestAppContext) -> (String, Option<String>) {
        self.view.read_with(cx, |view, _| {
            let mode = view.browser.compare_mode().expect("comparing");
            (
                mode.folder().to_owned(),
                mode.selected_path().map(str::to_owned),
            )
        })
    }

    fn stream_open(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.stream_probe().is_some())
    }
}

#[gpui_kit::test]
fn compare_stream_alt_f_opens_every_difference_under_the_folder(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.send_rows(cx, vec![row(3, "same.txt", RowStatus::Same)]);
    rig.press(cx, "alt-f");
    assert_eq!(rig.item_paths(cx), ["a.txt", "b.txt"], "Same rows stay out");
    let items = rig.diff.calls.items.lock().unwrap().clone();
    assert_eq!(items, vec![(1, 2)], "the app hears both items");
    rig.press(cx, "j");
    assert_eq!(rig.cursor(cx).item, 1, "the Stream has the keys");
}

#[gpui_kit::test]
fn compare_stream_enter_and_esc_round_trip_keeps_the_card(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.send_card(cx, 1, 2);
    rig.send_card(cx, 2, 2);
    for _ in 0..3 {
        rig.press(cx, "j");
    }
    assert_eq!(rig.cursor(cx), stop(1, 1));
    rig.press(cx, "enter");
    assert_eq!(rig.shown(cx), Some((2, "b.txt".into())));
    assert_eq!(
        rig.open_two_hunks(cx),
        Some(1),
        "the diff opens at the card's change"
    );
    rig.press(cx, "escape");
    assert_eq!(rig.shown(cx), None);
    assert_eq!(rig.cursor(cx), stop(1, 1), "Esc returns to the same card");
    rig.press(cx, "k");
    assert_eq!(rig.cursor(cx), stop(1, 0), "the Stream has the keys again");
}

#[gpui_kit::test]
fn compare_stream_next_file_from_the_diff_walks_the_stream(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.press(cx, "enter");
    assert_eq!(rig.shown(cx), Some((1, "a.txt".into())));
    rig.press(cx, "ctrl-alt-down");
    assert_eq!(rig.shown(cx), Some((2, "b.txt".into())));
    assert_eq!(rig.cursor(cx).item, 1, "the Stream's cursor follows");
    assert_eq!(
        rig.open_two_hunks(cx),
        Some(0),
        "a walked file opens at its first change"
    );
    rig.press(cx, "ctrl-alt-down");
    assert_eq!(
        rig.shown(cx),
        Some((2, "b.txt".into())),
        "stops at the last file"
    );
    rig.press(cx, "ctrl-alt-up");
    assert_eq!(rig.shown(cx), Some((1, "a.txt".into())));
}

#[gpui_kit::test]
fn compare_stream_next_file_from_a_pane_diff_walks_the_items(cx: &mut TestAppContext) {
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
    rig.press(cx, "ctrl-alt-down");
    rig.press(cx, "ctrl-alt-down");
    assert_eq!(
        rig.shown(cx),
        Some((4, "d.txt".into())),
        "the Same file is skipped"
    );
    rig.press(cx, "escape");
    assert_eq!(rig.compare_place(cx), (String::new(), Some("d.txt".into())));
}

#[gpui_kit::test]
fn compare_stream_leaving_reveals_a_row_two_folders_down(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.fs.add_folder("home/docs/deep");
    rig.fs.add_file("home/docs/deep/c.txt", 3);
    let deep = vec![
        row(3, "docs", RowStatus::Different),
        row(4, "docs/deep", RowStatus::Different),
        row(5, "docs/deep/c.txt", RowStatus::Different),
    ];
    rig.send_rows(cx, deep);
    rig.press(cx, "alt-f");
    assert_eq!(rig.item_paths(cx), ["a.txt", "b.txt", "docs/deep/c.txt"]);
    rig.press(cx, "shift-j");
    rig.press(cx, "shift-j");
    assert_eq!(rig.cursor(cx).item, 2);
    rig.press(cx, "escape");
    assert!(!rig.stream_open(cx), "Esc leaves the Stream");
    let place = (String::from("docs/deep"), Some("docs/deep/c.txt".into()));
    assert_eq!(rig.compare_place(cx), place);
}

#[gpui_kit::test]
fn compare_stream_rows_that_change_refresh_the_items_and_keep_the_cursor(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "alt-f");
    rig.press(cx, "shift-j");
    assert_eq!(rig.cursor(cx).item, 1);
    rig.send_rows(
        cx,
        vec![
            row(0, "0.txt", RowStatus::Checking),
            row(1, "a.txt", RowStatus::Same),
        ],
    );
    rig.frame(cx);
    assert_eq!(
        rig.item_paths(cx),
        ["b.txt", "0.txt"],
        "a.txt settled Same and left"
    );
    let at = rig.cursor(cx).item;
    assert_eq!(
        rig.item_paths(cx)[at],
        "b.txt",
        "the cursor stays on its file"
    );
    let items = rig.diff.calls.items.lock().unwrap().clone();
    assert_eq!(
        items.last(),
        Some(&(1, 2)),
        "the app hears the new items under the same generation"
    );
}

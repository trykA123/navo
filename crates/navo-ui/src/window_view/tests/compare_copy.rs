//! Copy one change (V27): Ctrl+Alt+←/→ and the hover bar copy through the fake `DiffHooks`, a
//! change with no digest offers no bar and the key says why, and Ctrl+Z runs `files.undo`.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_l10n::tr;

use super::compare_screen::{Rig, rig};
use crate::compare_diff::{
    CopyDirection, CopyDone, CopyResult, DiffRow, DiffUpdate, HunkView, PairFacts, PairKind,
    PairText, ViewedKey,
};
use crate::window_view::compare_screen::FileState;

mod menu;
mod pick;
mod running;
mod stream;

/// The digest of the one copyable change.
const DIGEST: [u8; 16] = [7; 16];

/// A hunk of one changed row `at`, with a digest when `copyable`.
fn hunk(at: u32, copyable: bool) -> HunkView {
    HunkView {
        rows: at..at + 1,
        left_bytes: u64::from(at) * 10..u64::from(at) * 10 + 4,
        right_bytes: u64::from(at) * 10..u64::from(at) * 10 + 5,
        digest: copyable.then_some(DIGEST),
        added: 1,
        removed: 1,
    }
}

/// A 40-row pair: change 0 at row 3 can be copied, change 1 at row 8 has no digest.
fn text() -> PairText {
    let rows = (0..40)
        .map(|at| match at {
            3 | 8 => DiffRow::Changed(at, at),
            _ => DiffRow::Equal(at, at),
        })
        .collect();
    let lines = vec!["line".to_owned(); 40];
    PairText {
        left_starts: Vec::new(),
        left: lines.clone(),
        right: lines,
        rows,
        hunks: vec![hunk(3, true), hunk(8, false)],
        facts: PairFacts {
            left_encoding: "compare-encoding-utf-8",
            right_encoding: "compare-encoding-utf-8",
            left_endings: "compare-endings-lf",
            right_endings: "compare-endings-lf",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    }
}

impl Rig {
    /// Opens the selected row as the text pair and draws it.
    fn open_pair(&self, cx: &mut TestAppContext) {
        self.open_with(cx, text());
    }

    /// Opens the selected row (or reloads the open pair) with `text` and draws it.
    fn open_with(&self, cx: &mut TestAppContext, text: PairText) {
        if self.diff.requests().is_empty() {
            self.press(cx, "enter");
        }
        let opened = DiffUpdate::Opened {
            request: 1,
            kind: PairKind::Text,
            text: Some(Arc::new(text)),
        };
        self.diff.pair_sink()(opened);
        self.frame(cx);
    }

    /// The last copy's note as (text, has Undo, is an error).
    fn note(&self, cx: &mut TestAppContext) -> Option<(String, bool, bool)> {
        self.view.read_with(cx, |view, _| view.copy_note())
    }

    /// Whether an element with `id` is on screen.
    fn has(&self, cx: &mut TestAppContext, id: (&'static str, usize)) -> bool {
        cx.update_window(self.window, |_, window, _| window.try_find(id).is_some())
            .expect("window")
    }

    /// Moves the pointer onto the element `id` and draws.
    fn hover(&self, cx: &mut TestAppContext, id: (&'static str, usize)) {
        cx.update_window(self.window, |_, window, cx| window.hover(id, cx))
            .expect("window");
        self.frame(cx);
    }

    /// Clicks the element `id` and draws.
    fn click(&self, cx: &mut TestAppContext, id: (&'static str, usize)) {
        cx.update_window(self.window, |_, window, cx| window.click(id, cx))
            .expect("window");
        self.frame(cx);
    }

    /// The app reports the end of the copy it was asked for.
    fn finish(&self, cx: &mut TestAppContext, direction: CopyDirection, result: CopyResult) {
        self.finish_request(cx, 1, (direction, result));
    }

    /// The app reports the end of the copy of pair `request`.
    fn finish_request(
        &self,
        cx: &mut TestAppContext,
        request: u64,
        (direction, result): (CopyDirection, CopyResult),
    ) {
        let done = CopyDone {
            request,
            direction,
            result,
        };
        self.diff.copy_sink()(done);
        self.frame(cx);
    }
}

#[gpui_kit::test]
fn compare_copy_ctrl_alt_right_copies_the_current_change_through_the_fake(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    let one = &asked[0];
    assert_eq!(one.direction, CopyDirection::ToRight);
    assert_eq!((one.pair.id, one.digest), (1, DIGEST));
    assert_eq!((&one.left_bytes, &one.right_bytes), (&(30..34), &(30..35)));
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies().len(), 1, "one copy at a time");
}

#[gpui_kit::test]
fn compare_copy_ctrl_alt_left_copies_to_the_left(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-left");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToLeft);
}

#[gpui_kit::test]
fn compare_copy_a_copied_change_says_so_with_undo_and_the_pair_reloads(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.note(cx), None, "no note while the copy runs");
    rig.finish(cx, CopyDirection::ToRight, CopyResult::Copied);
    let said = tr("compare-copied-right");
    assert_eq!(rig.note(cx), Some((said, true, false)));
    assert_eq!(rig.diff.refreshed(), 1, "the pair reloads");
    let opened = DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Text,
        text: Some(Arc::new(text())),
    };
    rig.diff.pair_sink()(opened);
    rig.frame(cx);
    let (current, _, _) = rig
        .view
        .update(cx, |view, _| {
            view.file_probe(crate::compare_diff::Layout::SideBySide)
        })
        .expect("an opened pair");
    assert_eq!(
        current,
        Some(0),
        "the next change takes the copied one's place"
    );
    rig.press(cx, "alt-down");
    assert_eq!(rig.note(cx), None, "moving on clears the note");
}

#[gpui_kit::test]
fn compare_copy_a_refusal_shows_its_text_and_reloads_nothing(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-left");
    let why = tr("compare-copy-changed");
    rig.finish(cx, CopyDirection::ToLeft, CopyResult::Refused(why.clone()));
    assert_eq!(rig.note(cx), Some((why, false, true)));
    assert_eq!(rig.diff.refreshed(), 0, "a refusal reloads nothing");
    rig.press(cx, "ctrl-alt-left");
    assert_eq!(rig.diff.copies().len(), 2, "a new copy may follow");
}

#[gpui_kit::test]
fn compare_copy_a_change_with_no_digest_offers_no_bar_and_the_key_says_why(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    assert!(!rig.has(cx, ("compare-file-row", 8)), "no hover wrapper");
    assert!(
        rig.has(cx, ("compare-file-row", 3)),
        "the copyable change has one"
    );
    rig.press(cx, "alt-down");
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-right");
    assert!(rig.diff.copies().is_empty(), "nothing was asked");
    let said = tr("compare-copy-no-digest");
    assert_eq!(rig.note(cx), Some((said, false, true)));
}

#[gpui_kit::test]
fn compare_copy_the_key_without_a_current_change_asks_to_pick_one(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "ctrl-alt-right");
    assert!(rig.diff.copies().is_empty());
    assert_eq!(rig.note(cx), Some((tr("compare-copy-pick"), false, true)));
}

#[gpui_kit::test]
fn compare_copy_the_hover_bar_appears_on_a_copyable_change_and_its_buttons_copy(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    assert!(
        !rig.has(cx, ("compare-copy-right", 3)),
        "no bar until hovered"
    );
    rig.hover(cx, ("compare-file-row", 3));
    assert!(rig.has(cx, ("compare-copy-left", 3)));
    assert!(rig.has(cx, ("compare-copy-right", 3)));
    rig.click(cx, ("compare-copy-left", 3));
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToLeft);
    assert_eq!(asked[0].digest, DIGEST);
}

#[gpui_kit::test]
fn compare_copy_ctrl_z_runs_files_undo(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "ctrl-z");
    rig.frame(cx);
    let notice = rig.view.read_with(cx, |view, _| {
        view.status_notice().map(|n| n.text.to_string())
    });
    assert_eq!(notice, Some(tr("undo-nothing")), "files.undo answered");
    assert!(matches!(rig.state(cx), Some(FileState::Opened { .. })));
}

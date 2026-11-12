//! The plan sheet of copy all matches (V31): the keys need a finished search, the sheet lists
//! files and refusals, Enter runs once, Esc cancels, and the result line says what the job did.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_l10n::tr;

use super::compare_screen::{Rig, rig};
use super::screen_access::screen_read;
use crate::compare_diff::{
    Card, CardRow, CopyDirection, DiffRow, HunkView, PairKind, SheetPlan, SheetResult, SheetRow,
    StreamUpdate, ViewedKey,
};

/// A text card whose `hunks` changes all have a digest, so each can be copied.
fn card(hunks: u32) -> Arc<Card> {
    let mut rows = Vec::new();
    let mut views = Vec::new();
    for hunk in 0..hunks {
        let at = hunk * 10;
        rows.push(CardRow::Line {
            row: DiffRow::Changed(at, at),
            hunk,
            left: Some("left".into()),
            right: Some("right".into()),
        });
        views.push(HunkView {
            rows: at..at + 1,
            left_bytes: u64::from(at)..u64::from(at) + 4,
            right_bytes: u64::from(at)..u64::from(at) + 5,
            digest: Some([hunk as u8; 16]),
            added: 1,
            removed: 1,
        });
    }
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

/// A sheet row of `path` that copies `hunks` changes, or is refused with `refusal`.
fn row(path: &str, hunks: usize, refusal: Option<&'static str>) -> SheetRow {
    SheetRow {
        path: path.to_owned(),
        hunks,
        bytes: 4 * hunks as u64,
        refusal,
    }
}

impl Rig {
    /// Sends the card of item `id` with three changes and draws.
    fn send_cards(&self, cx: &mut TestAppContext, id: u64) {
        self.diff.stream_sink()(StreamUpdate::Card {
            generation: 1,
            id,
            card: card(3),
        });
        self.frame(cx);
    }

    /// Types `text` into the focused element and draws.
    fn type_in(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        self.frame(cx);
    }

    /// Lets `ms` milliseconds pass and draws.
    fn pause(&self, cx: &mut TestAppContext, ms: u64) {
        cx.executor().advance_clock(Duration::from_millis(ms));
        self.frame(cx);
    }

    /// The app answers the search with `hits`; `done` ends it.
    fn hits(
        &self,
        cx: &mut TestAppContext,
        (query, done): (u64, bool),
        hits: Vec<(u64, Vec<u32>)>,
    ) {
        self.diff.stream_sink()(StreamUpdate::Hits {
            generation: 1,
            query,
            hits,
            searched: 3,
            total: 3,
            done,
        });
        self.frame(cx);
    }

    /// The status line's note as (text, has Undo, is an error).
    fn last_note(&self, cx: &mut TestAppContext) -> Option<(String, bool, bool)> {
        screen_read(&self.view, cx, |view, _| view.copy_note())
    }

    /// Opens a Stream of three files; cards 1 and 3 have three changes, card 2 is not sent.
    /// The search `ab` found change 0 of file 1 and changes 1 and 2 of files 2 and 3.
    fn open_matches(&self, cx: &mut TestAppContext, done: bool) {
        self.open_stream(cx, 3);
        for id in [1, 3] {
            self.send_cards(cx, id);
        }
        self.press(cx, "/");
        self.type_in(cx, "ab");
        self.pause(cx, 150);
        let hits = vec![(1, vec![0]), (2, vec![1]), (3, vec![1, 2])];
        self.hits(cx, (1, done), hits);
        self.press(cx, "enter");
    }

    /// The sheet's lines, or `None` while it is closed.
    fn sheet(&self, cx: &mut TestAppContext) -> Option<Vec<String>> {
        screen_read(&self.view, cx, |view, _| view.sheet_lines())
    }

    /// The app plans: file 1 is written, file 3 changed since it was shown.
    fn plan(&self, cx: &mut TestAppContext) {
        let rows = vec![
            row("f001.txt", 1, None),
            row("f003.txt", 0, Some("compare-copy-changed")),
        ];
        self.diff.sheet_plan_sink()(SheetPlan { rows });
        self.frame(cx);
    }
}

#[gpui_kit::test]
fn compare_sheet_keys_need_a_finished_search(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 3);
    rig.send_cards(cx, 1);
    rig.press(cx, "ctrl-shift-right");
    let said = (tr("compare-sheet-need-search"), false, true);
    assert_eq!(rig.last_note(cx), Some(said));
    rig.press(cx, "/");
    rig.type_in(cx, "ab");
    rig.pause(cx, 150);
    rig.hits(cx, (1, false), vec![(1, vec![0])]);
    rig.press(cx, "enter");
    rig.press(cx, "ctrl-shift-left");
    assert_eq!(
        rig.last_note(cx),
        Some((tr("compare-sheet-wait"), false, true))
    );
    assert!(rig.diff.sheets().is_empty(), "no plan was asked");
}

#[gpui_kit::test]
fn compare_sheet_asks_for_every_matched_change_and_lists_refusals(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    let asked = rig.diff.sheets();
    assert_eq!(asked.len(), 1);
    // Files 1 and 3 give 1 + 2 changes; file 2 has no card yet and is left out by the UI.
    assert_eq!(asked[0].len(), 3);
    assert!(
        asked[0]
            .iter()
            .all(|c| c.direction == CopyDirection::ToRight)
    );
    assert_eq!(rig.sheet(cx), None, "the sheet opens when the plan arrives");
    rig.plan(cx);
    let lines = rig.sheet(cx).expect("the sheet is open");
    assert_eq!(lines.len(), 3);
    let why = |id| tr(id);
    assert!(
        lines[0].contains("f002.txt") && lines[0].contains(&why("compare-sheet-why-not-loaded"))
    );
    assert!(lines[1].contains("f003.txt") && lines[1].contains(&why("compare-sheet-why-changed")));
    assert!(lines[2].contains("f001.txt") && !lines[2].contains("Skipped"));
}

#[gpui_kit::test]
fn compare_sheet_enter_runs_one_job_and_the_result_line_names_it(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-left");
    assert_eq!(rig.diff.sheets()[0][0].direction, CopyDirection::ToLeft);
    rig.plan(cx);
    rig.press(cx, "enter");
    assert_eq!(rig.diff.sheet_runs(), 1);
    assert_eq!(rig.sheet(cx), None, "the sheet closes");
    rig.press(cx, "enter");
    assert_eq!(rig.diff.sheet_runs(), 1, "a second Enter runs nothing");
    rig.diff.sheet_sink()(SheetResult::Copied { files: 1, bytes: 4 });
    rig.frame(cx);
    let (text, undo, error) = rig.last_note(cx).expect("a note");
    assert!(text.contains("Copied") && text.contains("1 file") && text.contains("to the left"));
    assert!(undo && !error, "Undo follows the line");
}

#[gpui_kit::test]
fn compare_sheet_escape_cancels_without_a_run(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    rig.plan(cx);
    assert!(rig.sheet(cx).is_some());
    rig.press(cx, "escape");
    assert_eq!(rig.sheet(cx), None);
    assert_eq!(rig.diff.sheet_runs(), 0);
    rig.press(cx, "ctrl-shift-right");
    assert_eq!(rig.diff.sheets().len(), 2, "the keys work again");
}

#[gpui_kit::test]
fn compare_sheet_a_refused_job_shows_its_text_without_undo(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    rig.plan(cx);
    rig.press(cx, "enter");
    let text = tr("compare-copy-changed");
    rig.diff.sheet_sink()(SheetResult::Refused(text.clone()));
    rig.frame(cx);
    assert_eq!(rig.last_note(cx), Some((text, false, true)));
}

impl Rig {
    /// Whether a single-copy key is held off by the "copy running" note.
    fn copy_key_waits(&self, cx: &mut TestAppContext) -> bool {
        self.press(cx, "ctrl-alt-right");
        let running = tr("compare-copy-running");
        self.last_note(cx)
            .is_some_and(|(text, _, _)| text == running)
    }
}

#[gpui_kit::test]
fn compare_sheet_holds_single_copies_until_it_is_cancelled(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    assert!(rig.copy_key_waits(cx), "held while the plan runs");
    rig.plan(cx);
    assert!(rig.copy_key_waits(cx), "held while the sheet is open");
    rig.press(cx, "escape");
    assert!(!rig.copy_key_waits(cx), "Esc frees the copy keys");
}

#[gpui_kit::test]
fn compare_sheet_a_plan_of_refusals_only_frees_the_copy_keys_on_enter(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    rig.diff.sheet_plan_sink()(SheetPlan::default());
    rig.frame(cx);
    assert!(rig.sheet(cx).is_some(), "the UI's own refusals are listed");
    rig.press(cx, "enter");
    assert_eq!(rig.diff.sheet_runs(), 0, "nothing to run");
    assert!(!rig.copy_key_waits(cx));
}

#[gpui_kit::test]
fn compare_sheet_a_refused_job_frees_the_copy_keys_and_reloads_nothing(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_matches(cx, true);
    rig.press(cx, "ctrl-shift-right");
    rig.plan(cx);
    rig.press(cx, "enter");
    assert!(rig.copy_key_waits(cx), "held while the job runs");
    rig.diff.sheet_sink()(SheetResult::Refused(tr("compare-copy-changed")));
    rig.frame(cx);
    let generation = screen_read(&rig.view, cx, |view, _| {
        view.stream_probe().expect("a Stream").0.generation()
    });
    assert_eq!(generation, 1, "a refusal rebuilds no card");
    assert!(!rig.copy_key_waits(cx));
}

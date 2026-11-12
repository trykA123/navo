//! Copying from the Stream (V27): the key and the hover bar copy a card's change, and the end
//! of a copy rebuilds that one card.

use std::sync::Arc;

use gpui_kit::TestAppContext;
use navo_l10n::tr;

use super::super::compare_screen::{Rig, rig};
use super::super::screen_access::screen_read;
use super::{DIGEST, hunk};
use crate::compare_diff::{
    Card, CardRow, CopyDirection, CopyResult, DiffRow, PairKind, StreamUpdate, ViewedKey,
};

/// A Stream card of one copyable change.
fn card() -> Arc<Card> {
    let line = |row: DiffRow, hunk: u32| CardRow::Line {
        row,
        hunk,
        left: Some("left".into()),
        right: Some("right".into()),
    };
    Arc::new(Card {
        kind: PairKind::Text,
        rows: vec![
            line(DiffRow::Equal(0, 0), 0),
            line(DiffRow::Changed(1, 1), 0),
        ],
        hunks: vec![hunk(1, true)],
        added: 1,
        removed: 1,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    })
}

impl Rig {
    /// Opens a one-item Stream and sends item 1 its card of one copyable change.
    pub(super) fn open_stream_card(&self, cx: &mut TestAppContext) {
        self.open_stream(cx, 1);
        let sink = self.diff.stream_sink();
        sink(StreamUpdate::Card {
            generation: 1,
            id: 1,
            card: card(),
        });
        self.frame(cx);
    }

    /// The list row of the first line of the open Stream's copyable change.
    fn card_row_with_bar(&self, cx: &mut TestAppContext) -> usize {
        screen_read(&self.view, cx, |view, _| {
            let (_, rows, _) = view.stream_probe().expect("a Stream");
            rows.iter()
                .position(|row| matches!(row, crate::compare_diff::StreamRow::Line { .. }))
                .expect("a card line")
        })
    }
}

#[gpui_kit::test]
fn compare_copy_the_stream_key_copies_the_current_change_and_reloads_its_card(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream_card(cx);
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].pair.id, asked[0].digest), (1, DIGEST));
    assert_eq!(asked[0].direction, CopyDirection::ToRight);
    rig.finish(cx, CopyDirection::ToRight, CopyResult::Copied);
    assert_eq!(
        rig.note(cx),
        Some((tr("compare-copied-right"), true, false))
    );
    assert_eq!(rig.diff.reloaded(), vec![1], "the copied card is rebuilt");
    let generation = screen_read(&rig.view, cx, |view, _| {
        view.stream_probe().expect("a Stream").0.generation()
    });
    assert_eq!(generation, 1, "no other card is rebuilt");
}

#[gpui_kit::test]
fn compare_copy_the_stream_hover_bar_copies_its_change(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream_card(cx);
    assert!(
        !rig.has(cx, ("compare-copy-right", 3)),
        "no bar until hovered"
    );
    let row = rig.card_row_with_bar(cx);
    rig.hover(cx, ("compare-card-row", row));
    rig.click(cx, ("compare-copy-right", row));
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToRight);
}

#[gpui_kit::test]
fn compare_copy_a_stream_refusal_reloads_no_card(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream_card(cx);
    rig.press(cx, "ctrl-alt-right");
    let why = tr("compare-copy-changed");
    rig.finish(cx, CopyDirection::ToRight, CopyResult::Refused(why));
    assert!(rig.diff.reloaded().is_empty(), "a refusal reloads nothing");
}

#[gpui_kit::test]
fn compare_copy_the_end_reloads_only_the_card_it_copied(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 3);
    for id in 1..=3 {
        rig.diff.stream_sink()(StreamUpdate::Card {
            generation: 1,
            id,
            card: card(),
        });
    }
    rig.frame(cx);
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    let id = asked[0].pair.id;
    rig.finish_request(cx, id, (CopyDirection::ToRight, CopyResult::Copied));
    assert_eq!(rig.diff.reloaded(), vec![id], "one card, not all three");
}

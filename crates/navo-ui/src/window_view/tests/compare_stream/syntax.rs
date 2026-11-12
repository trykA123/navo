//! The Stream parses each text card off the UI thread (D38.3): the parse lands from the background
//! job and colours the card's lines, a plain pair gets no runs, and a card that is replaced,
//! evicted or closed holds no parse — a parse of a card that is gone is dropped on arrival.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::compare_screen::{Rig, rig};
use super::entry::row;
use crate::browser::{CompareUpdate, RowStatus};
use crate::compare_diff::card::syntax::{CardKey, CardLook, CardSyntax, TextRun};
use crate::compare_diff::{
    Card, CardRow, DiffRow, HunkView, PairKind, PairRequest, StreamItem, StreamList, StreamUpdate,
    ViewedKey,
};
use crate::tokens::{Mode, Theme, highlight_theme};
use crate::window_view::EDIT_LIMIT;

/// The card's Rust lines: a comment, a keyword line and a closing brace.
const CODE: [&str; 3] = ["// note", "fn main() {}", "}"];

/// Opens a Stream over one file named `name` and draws it.
pub(super) fn open_one(rig: &Rig, cx: &mut TestAppContext, name: &str) {
    let list = StreamList {
        items: vec![StreamItem {
            id: 1,
            path: name.to_owned(),
            status: RowStatus::Different,
            settled: true,
        }],
        rules: 0,
    };
    let requests = vec![PairRequest {
        id: 1,
        path: name.to_owned(),
        left: None,
        right: None,
    }];
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_stream((list, requests), window, cx)
        })
    })
    .expect("window");
    rig.frame(cx);
}

/// One card line with the same text on both sides.
fn line(row: DiffRow, hunk: u32, text: &str) -> CardRow {
    CardRow::Line {
        row,
        hunk,
        left: Some(text.to_owned()),
        right: Some(text.to_owned()),
    }
}

/// A text card of `hunks` hunks, each the three `CODE` lines on both sides.
pub(super) fn code_card(hunks: u32) -> Arc<Card> {
    let rows = (0..hunks)
        .flat_map(|hunk| {
            [
                line(DiffRow::Equal(hunk, hunk), hunk, CODE[0]),
                line(DiffRow::Changed(hunk, hunk), hunk, CODE[1]),
                line(DiffRow::Equal(hunk, hunk), hunk, CODE[2]),
            ]
        })
        .collect();
    let view = |hunk: u32| HunkView {
        rows: hunk * 3..hunk * 3 + 3,
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: None,
        added: 1,
        removed: 1,
    };
    Arc::new(Card {
        kind: PairKind::Text,
        rows,
        hunks: (0..hunks).map(view).collect(),
        added: hunks,
        removed: hunks,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    })
}

/// Sends `card` for item 1 under `generation`, as the app would.
pub(super) fn send_card(rig: &Rig, generation: u64, card: &Arc<Card>) {
    (rig.diff.stream_sink())(StreamUpdate::Card {
        generation,
        id: 1,
        card: Arc::clone(card),
    });
}

/// Evicts item 1's card, as the app would when it falls far from the view.
fn evict(rig: &Rig) {
    (rig.diff.stream_sink())(StreamUpdate::Evicted {
        generation: 1,
        id: 1,
    });
}

/// The left colour runs the Stream paints for row `at` of `card`; empty with no parse held.
pub(super) fn held_runs(rig: &Rig, cx: &mut TestAppContext, card: &Card, at: u32) -> Vec<TextRun> {
    let Some(held) = rig.view.read_with(cx, |view, _| view.stream_syntax(1)) else {
        return Vec::new();
    };
    let t = Theme::new(Mode::Light);
    let colours = highlight_theme(
        t.syntax,
        t.mode,
        &t.colors,
        &HighlightTheme::default_light(),
    );
    let look = CardLook::new(Some(&held), Some(&colours)).expect("a card look");
    look.line_runs(&t, &card.rows, at).0
}

/// The byte range of the coloured keyword at the start of the line, if the runs hold one.
pub(super) fn keyword_at(runs: &[TextRun]) -> Option<Range<usize>> {
    runs.iter()
        .find(|(run, style)| run.contains(&0) && style.color.is_some())
        .map(|(run, _)| run.clone())
}

/// How many card parse slots the open Stream holds.
fn slots(rig: &Rig, cx: &mut TestAppContext) -> usize {
    rig.view.read_with(cx, |view, _| view.stream_syntax_slots())
}

/// Drains the Stream's inbox and waits for the background parse, as the file screen's test does.
pub(super) fn land(rig: &Rig, cx: &mut TestAppContext) {
    rig.view.update(cx, |view, cx| view.host_poll(cx));
    cx.run_until_parked();
}

/// Delivers `parse` as the answer of a job started for `key`, as a background job does.
fn deliver(rig: &Rig, cx: &mut TestAppContext, key: CardKey, parse: CardSyntax) -> bool {
    rig.view
        .update(cx, |view, cx| view.deliver_stream_syntax(key, parse, cx))
}

#[gpui_kit::test]
fn compare_stream_a_rust_card_colours_its_lines_after_the_parse_lands(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_one(&rig, cx, "main.rs");
    let card = code_card(1);
    send_card(&rig, 1, &card);
    rig.view.update(cx, |view, cx| view.host_poll(cx));
    assert!(
        held_runs(&rig, cx, &card, 1).is_empty(),
        "the poll returns before the background parse ends"
    );
    land(&rig, cx);
    let runs = held_runs(&rig, cx, &card, 1);
    assert!(
        keyword_at(&runs).is_some(),
        "the `fn` line of the card has a colour run once the parse lands: {runs:?}"
    );
}

#[gpui_kit::test]
fn compare_stream_a_plain_card_gets_no_runs(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_one(&rig, cx, "notes.txt");
    let card = code_card(1);
    send_card(&rig, 1, &card);
    land(&rig, cx);
    assert!(
        rig.view
            .read_with(cx, |view, _| view.stream_syntax(1))
            .is_some(),
        "a plain card is parsed too; it only finds no grammar"
    );
    let runs = held_runs(&rig, cx, &card, 1);
    assert!(
        runs.is_empty(),
        "a `.txt` pair has no grammar, so its lines draw as before"
    );
}

#[gpui_kit::test]
fn compare_stream_a_replaced_card_drops_the_old_parse(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_one(&rig, cx, "main.rs");
    let old = code_card(2);
    send_card(&rig, 1, &old);
    land(&rig, cx);
    assert!(
        !held_runs(&rig, cx, &old, 4).is_empty(),
        "the second hunk of the held card colours"
    );
    // The app sends a new card for the same item: six lines become three.
    let fresh = code_card(1);
    send_card(&rig, 1, &fresh);
    land(&rig, cx);
    land(&rig, cx);
    assert!(
        held_runs(&rig, cx, &old, 4).is_empty(),
        "a replaced card never shows the old card's runs"
    );
    assert!(
        !held_runs(&rig, cx, &fresh, 1).is_empty(),
        "the new card colours from its own parse"
    );
    // The old parse, delivered now that its card is gone, is dropped (D38.3).
    let stale = CardSyntax::parse("main.rs", &old, EDIT_LIMIT);
    assert!(
        !deliver(&rig, cx, CardKey::new(1, 1, &old), stale),
        "a parse of a card that is gone is dropped"
    );
}

#[gpui_kit::test]
fn compare_stream_an_evicted_or_closed_stream_holds_no_parse(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_one(&rig, cx, "main.rs");
    let card = code_card(1);
    send_card(&rig, 1, &card);
    rig.frame(cx);
    assert_eq!(slots(&rig, cx), 1, "the held card has a slot");
    evict(&rig);
    land(&rig, cx);
    assert_eq!(slots(&rig, cx), 0, "an evicted card drops its slot");
    send_card(&rig, 1, &card);
    rig.frame(cx);
    assert_eq!(slots(&rig, cx), 1, "the card coming back is parsed again");
    rig.press(cx, "escape");
    assert_eq!(slots(&rig, cx), 0, "a closed Stream holds no parse");
}

#[gpui_kit::test]
fn compare_stream_a_new_generation_reparses_the_card_it_kept(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_one(&rig, cx, "main.rs");
    let card = code_card(1);
    send_card(&rig, 1, &card);
    land(&rig, cx);
    assert_eq!(
        slots(&rig, cx),
        1,
        "the card is parsed under the first generation"
    );
    // A new compare batch rebuilds the item list: the Stream moves on to generation 2 and keeps
    // the card it holds until the app sends a new one.
    (rig.compare)(CompareUpdate::Rows {
        generation: 2,
        rows: vec![row(1, "main.rs", RowStatus::Different)],
        progress: Vec::new(),
    });
    land(&rig, cx);
    assert_eq!(
        slots(&rig, cx),
        1,
        "the kept card is parsed again under the new generation"
    );
    assert!(
        !held_runs(&rig, cx, &card, 1).is_empty(),
        "and its lines keep colouring"
    );
}

/// Opens a Stream over two Rust files, `a.rs` (id 1) and `b.rs` (id 2), and draws it.
fn open_two_rs(rig: &Rig, cx: &mut TestAppContext) {
    let items = vec![
        StreamItem {
            id: 1,
            path: "a.rs".to_owned(),
            status: RowStatus::Different,
            settled: true,
        },
        StreamItem {
            id: 2,
            path: "b.rs".to_owned(),
            status: RowStatus::Different,
            settled: true,
        },
    ];
    let requests = items
        .iter()
        .map(|item| PairRequest {
            id: item.id,
            path: item.path.clone(),
            left: None,
            right: None,
        })
        .collect();
    let list = StreamList { items, rules: 0 };
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_stream((list, requests), window, cx)
        })
    })
    .expect("window");
    rig.frame(cx);
}

#[gpui_kit::test]
fn compare_stream_a_job_lands_on_the_right_card_after_an_item_was_inserted_before_it(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    open_two_rs(&rig, cx);
    let card = code_card(1);
    send_card(&rig, 1, &card);
    land(&rig, cx);
    assert!(
        !held_runs(&rig, cx, &card, 1).is_empty(),
        "a.rs colours before the insertion"
    );
    // A plain-text item arrives before a.rs under the same generation, so a.rs moves from index 0
    // to index 1 while the generation stays 1 (WR3).
    rig.view.update(cx, |view, _| {
        view.stream_insert_item_before(
            1,
            StreamItem {
                id: 3,
                path: "c.txt".to_owned(),
                status: RowStatus::Different,
                settled: true,
            },
        );
    });
    // A replacement card for a.rs needs a new job; it must still read a.rs, not the inserted c.txt.
    let fresh = code_card(1);
    send_card(&rig, 1, &fresh);
    land(&rig, cx);
    land(&rig, cx);
    assert!(
        !held_runs(&rig, cx, &fresh, 1).is_empty(),
        "the job still lands on a.rs after an item was inserted before it"
    );
}

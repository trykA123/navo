//! Heavy cards in the Stream (D26.5, D26.7): a large card is drawn at its fixed height from the
//! moment it arrives, its detail redraws that same row, and the card stays one J stop (D27).

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Pixels, TestAppContext, px};

use super::super::compare_screen::{Rig, rig};
use crate::compare_diff::card::{LINE_HEIGHT, NOTE_HEIGHT};
use crate::compare_diff::{
    BytesCard, CARD_HUNKS, Card, CardDetail, CardScan, HEX_CARD_ROWS, HexWindow, LargeCard,
    LargeHunk, LargeProgress, LargeStage, PairKind, Stop, StreamRow, StreamUpdate, ViewedKey,
};

/// A GiB per side: past the Whole tier, so the pair is a large one (D8).
const GI: u64 = 1 << 30;

/// The large pair kind these tests draw.
fn large() -> PairKind {
    PairKind::Large {
        left: GI,
        right: GI,
    }
}

/// The base card of a heavy pair: a summary with no rows of its own (D26.5).
fn heavy_card(kind: PairKind) -> Arc<Card> {
    Arc::new(Card {
        kind,
        rows: Vec::new(),
        hunks: Vec::new(),
        added: 0,
        removed: 0,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    })
}

/// A large detail of `hunks` one-line ranges, the first at line `base` (D26.4 snapshot).
fn large_detail(hunks: u32, base: u64) -> Arc<CardDetail> {
    let hunk = |at: u64| LargeHunk {
        left_rows: at..at + 1,
        right_rows: at..at + 1,
        left_bytes: 0..0,
        right_bytes: 0..0,
    };
    let rows = u64::from(hunks);
    Arc::new(CardDetail::Large(LargeCard {
        hunks: (0..rows).map(|at| hunk(base + at * 10)).collect(),
        found: hunks,
        progress: Some(LargeProgress {
            stage: LargeStage::Scan,
            done: rows,
            total: 100,
        }),
        scan: CardScan::Running,
    }))
}

/// The height D26.5 fixes for a large or image card, from the same constants the view uses.
fn card_height() -> Pixels {
    px(NOTE_HEIGHT + CARD_HUNKS as f32 * LINE_HEIGHT)
}

impl Rig {
    /// Sends the base card of a heavy pair for item `id`, as the app's card workers do.
    pub(super) fn send_heavy(&self, cx: &mut TestAppContext, id: u64, kind: PairKind) {
        let sink = self.diff.stream_sink();
        sink(StreamUpdate::Card {
            generation: 1,
            id,
            card: heavy_card(kind),
        });
        self.frame(cx);
    }

    /// Sends item `id`'s detail, as the session's one detail lane would (D26.1).
    pub(super) fn send_detail(&self, cx: &mut TestAppContext, id: u64, detail: Arc<CardDetail>) {
        let sink = self.diff.stream_sink();
        sink(StreamUpdate::Detail {
            generation: 1,
            id,
            detail,
        });
        self.frame(cx);
    }

    /// The index of the first row equal to `wanted`.
    fn row_index(&self, cx: &mut TestAppContext, wanted: StreamRow) -> usize {
        self.view.read_with(cx, |view, _| {
            let (_, rows, _) = view.stream_probe().expect("a Stream");
            rows.iter().position(|row| *row == wanted).expect("the row")
        })
    }

    /// The Stream's rows, to show a detail splices none.
    fn rows(&self, cx: &mut TestAppContext) -> Vec<StreamRow> {
        self.view.read_with(cx, |view, _| {
            let (_, rows, _) = view.stream_probe().expect("a Stream");
            rows.to_vec()
        })
    }

    /// The height the list gave row `at`; the row must be drawn for the list to know it.
    fn row_height(&self, cx: &mut TestAppContext, at: usize) -> Pixels {
        self.view.read_with(cx, |view, _| {
            let (_, _, list) = view.stream_probe().expect("a Stream");
            list.bounds_for_item(at).expect("a drawn row").size.height
        })
    }

    /// The height D26.5 fixes for a binary card, read from the theme the app draws with.
    fn binary_height(&self, cx: &mut TestAppContext) -> Pixels {
        self.view.read_with(cx, |_, cx| {
            let t = crate::tokens::current(cx);
            px(NOTE_HEIGHT) + t.spacing.row_height * HEX_CARD_ROWS as f32
        })
    }

    /// Whether the last frame drew hex side cell `index` inside item `id`'s detail row.
    fn hex_cell(&self, cx: &mut TestAppContext, id: u64, index: usize) -> bool {
        cx.update_window(self.window, |_, window, _| {
            window
                .within(("compare-card-detail", id))
                .try_find(("paged-row", index))
                .is_some()
        })
        .expect("window")
    }

    /// The text the last frame drew on hex side cell `index` of item `id`'s detail row.
    fn hex_label(&self, cx: &mut TestAppContext, id: u64, index: usize) -> Option<String> {
        cx.update_window(self.window, |_, window, _| {
            window
                .within(("compare-card-detail", id))
                .try_find(("paged-row", index))
                .and_then(|cell| cell.label().map(str::to_owned))
        })
        .expect("window")
    }

    /// The text the last frame drew on hunk row `n` of item `id`'s card, `None` when it drew none.
    fn hunk_text(&self, cx: &mut TestAppContext, id: u64, n: usize) -> Option<String> {
        cx.update_window(self.window, |_, window, _| {
            window
                .within(("compare-card-detail", id))
                .try_find(("compare-card-large-hunk", n))
                .and_then(|row| row.label().map(str::to_owned))
        })
        .expect("window")
    }
}

#[gpui_kit::test]
fn compare_stream_a_large_card_keeps_its_height_when_hunks_arrive(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_heavy(cx, 1, large());
    let at = rig.row_index(cx, StreamRow::Detail(0));
    let rows = rig.rows(cx);
    // A heavy card draws its full body height before any detail has arrived (D26.5).
    let before = rig.row_height(cx, at);
    assert_eq!(
        before,
        card_height(),
        "the empty heavy card is already full height"
    );
    assert_eq!(rig.hunk_text(cx, 1, 0), None, "and shows no ranges yet");
    rig.send_detail(cx, 1, large_detail(3, 0));
    assert_eq!(
        rig.rows(cx),
        rows,
        "the arriving detail splices no row (D26.3)"
    );
    assert_eq!(
        rig.row_height(cx, at),
        before,
        "and never changes the height"
    );
    // The row the list kept is redrawn with what the scan found (D26.7).
    assert_eq!(
        rig.hunk_text(cx, 1, 0).as_deref(),
        Some("Lines 1 → 1"),
        "the first range shows up"
    );
    assert_eq!(
        rig.hunk_text(cx, 1, 2).as_deref(),
        Some("Lines 21 → 21"),
        "and so does the third"
    );
    rig.send_detail(cx, 1, large_detail(3, 1));
    assert_eq!(
        rig.hunk_text(cx, 1, 0).as_deref(),
        Some("Lines 2 → 2"),
        "a later snapshot redraws the same row"
    );
    assert_eq!(rig.row_height(cx, at), before, "still no reflow");
}

#[gpui_kit::test]
fn compare_stream_a_large_card_is_one_j_stop(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_heavy(cx, 1, large());
    rig.send_detail(cx, 1, large_detail(CARD_HUNKS as u32, 0));
    assert_eq!(
        rig.cursor(cx),
        Stop {
            item: 0,
            hunk: None
        },
        "a card holding six ranges is still one stop (D27)"
    );
    rig.press(cx, "j");
    assert_eq!(
        rig.cursor(cx),
        Stop {
            item: 1,
            hunk: None
        },
        "J goes to the next card at once"
    );
}

/// A binary detail of `rows` full hex rows on both sides, one changed region at byte 0x40.
fn bytes_detail(rows: usize) -> Arc<CardDetail> {
    let len = rows * 16;
    Arc::new(CardDetail::Bytes(BytesCard {
        window: Some(HexWindow {
            left_start: 0,
            right_start: 0,
            left: vec![b'a'; len],
            right: vec![b'b'; len],
            changed_left: 0x40..0x42,
            changed_right: 0x40..0x42,
            wide: (false, false),
        }),
        progress: None,
        scan: CardScan::Full,
    }))
}

#[gpui_kit::test]
fn compare_stream_a_hex_card_has_nine_rows_at_a_fixed_height(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream(cx, 2);
    rig.send_heavy(cx, 1, PairKind::Binary);
    let at = rig.row_index(cx, StreamRow::Detail(0));
    let rows = rig.rows(cx);
    // The card is nine hex rows tall before any detail has arrived (D26.5).
    let want = rig.binary_height(cx);
    let before = rig.row_height(cx, at);
    assert_eq!(before, want, "a binary card keeps the hex height");
    assert!(!rig.hex_cell(cx, 1, 0), "and draws no hex row yet");
    rig.send_detail(cx, 1, bytes_detail(HEX_CARD_ROWS));
    assert_eq!(
        rig.rows(cx),
        rows,
        "the arriving window splices no row (D26.3)"
    );
    assert_eq!(
        rig.row_height(cx, at),
        before,
        "and never changes the height"
    );
    // Each of the nine rows draws both sides, tinted where the bytes differ (D26.8).
    for index in 0..HEX_CARD_ROWS * 2 {
        assert!(rig.hex_cell(cx, 1, index), "hex side cell {index} is drawn");
    }
    assert!(
        rig.hex_label(cx, 1, 0)
            .is_some_and(|label| label.starts_with("00000000")),
        "the row shows its offset: {:?}",
        rig.hex_label(cx, 1, 0)
    );
}

use std::sync::Arc;

use navo_ui::browser::{
    BarView, CompareFilter, CompareKind, CompareRow, CompareSide, RowStatus, RowView, SideView,
    Tone, Total,
};
use navo_ui::compare_diff::{
    Card, CardCtx, CardRow, DiffRow, Layout, PairKind, StreamModel, StreamUpdate, ViewedKey,
    stream_items, stream_row_view, stream_rows,
};
use navo_ui::compare_list::{CompareBar, CompareList};
use navo_ui::prelude::*;
use navo_ui::primitives::ListScroll;
use navo_ui::tokens;

use crate::compare_gutter_sample::gutter_sample;

struct Sample {
    name: &'static str,
    kind: CompareKind,
    sides: [Option<(&'static str, &'static str)>; 2],
    tone: Tone,
    status: &'static str,
    /// The left side has the later time, so its pane marks the row as the newer one.
    newer_left: bool,
}

const TIME: &str = "2026-10-20 13:41";
const EARLIER_TIME: &str = "2026-10-18 09:05";

const SAMPLES: [Sample; 10] = [
    sample(
        "docs",
        CompareKind::Folder,
        [Some(("<DIR>", TIME)); 2],
        Tone::Muted,
    ),
    sample(
        "Cargo.toml",
        CompareKind::File,
        [Some(("1.6 KB", TIME)), Some(("1.7 KB", EARLIER_TIME))],
        Tone::Change,
    )
    .said("different")
    .newer_on_left(),
    sample(
        "legacy_walk.rs",
        CompareKind::File,
        [Some(("5.3 KB", TIME)), None],
        Tone::Delete,
    )
    .said("only left"),
    sample(
        "write_file.rs",
        CompareKind::File,
        [None, Some(("6.7 KB", TIME))],
        Tone::Add,
    )
    .said("only right"),
    sample(
        "big_trace.asc",
        CompareKind::File,
        [Some(("612 MB", TIME)), Some(("612 MB", TIME))],
        Tone::Busy,
    )
    .said("same size and time · checking · 208.0 MB of 612.0 MB"),
    sample(
        "flash_dump.hex",
        CompareKind::File,
        [Some(("148 MB", TIME)), Some(("148 MB", TIME))],
        Tone::Muted,
    )
    .said("Not checked (stopped)"),
    sample(
        "notes",
        CompareKind::Folder,
        [Some(("<DIR>", TIME)), Some(("1.0 KB", TIME))],
        Tone::Conflict,
    )
    .said("type conflict"),
    sample(
        "secret.key",
        CompareKind::File,
        [Some(("64 B", TIME)), Some(("64 B", TIME))],
        Tone::Conflict,
    )
    .said("unavailable: access denied"),
    sample(
        "README.md",
        CompareKind::File,
        [Some(("5.9 KB", TIME)), Some(("5.9 KB", TIME))],
        Tone::Muted,
    )
    .said("same after rules"),
    sample(
        "lib.rs",
        CompareKind::File,
        [Some(("2.0 KB", TIME)), Some(("2.0 KB", TIME))],
        Tone::Muted,
    ),
];

const fn sample(
    name: &'static str,
    kind: CompareKind,
    sides: [Option<(&'static str, &'static str)>; 2],
    tone: Tone,
) -> Sample {
    Sample {
        name,
        kind,
        sides,
        tone,
        status: "",
        newer_left: false,
    }
}

impl Sample {
    const fn said(mut self, status: &'static str) -> Self {
        self.status = status;
        self
    }

    /// Mark this row's left side as the newer one of the pair.
    const fn newer_on_left(mut self) -> Self {
        self.newer_left = true;
        self
    }

    fn view(&self, side: usize) -> RowView {
        let here = self.sides[side].map(|(size, time)| SideView {
            kind: self.kind,
            size: size.to_owned(),
            time: time.to_owned(),
            newer: self.newer_left && side == 0,
        });
        let missing = if side == 0 {
            "not in left"
        } else {
            "not in right"
        };
        RowView {
            path: self.name.to_owned(),
            name: self.name.to_owned(),
            status: if here.is_some() {
                self.status.to_owned()
            } else {
                missing.to_owned()
            },
            side: here,
            tone: self.tone,
            gutter: None,
        }
    }
}

fn total(tone: Tone, text: &str) -> Total {
    Total {
        tone,
        text: text.to_owned(),
    }
}

fn bar() -> BarView {
    BarView {
        filter: CompareFilter::All,
        totals: vec![
            total(Tone::Change, "4 different"),
            total(Tone::Delete, "1 only left"),
            total(Tone::Add, "1 only right"),
            total(Tone::Muted, "3 same"),
            total(Tone::Busy, "1 checking"),
        ],
        running: Some("Comparing…".to_owned()),
        stopped: None,
    }
}

/// Two compared panes with a bar above them, sharing one scroll like the app does.
pub fn page(cx: &App) -> Div {
    let t = tokens::current(cx).clone();
    let scroll = ListScroll::new();
    let pane = |side: usize, id: &'static str| {
        let rows: Arc<[RowView]> = SAMPLES.iter().map(|sample| sample.view(side)).collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .rounded(t.radii.card)
            .overflow_hidden()
            .border_1()
            .border_color(t.surfaces.card_edge)
            .child(
                CompareList::new(id, rows, &scroll)
                    .selected(Some(1))
                    .active(side == 0),
            )
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(t.spacing.md)
        .p(t.spacing.md)
        .child(CompareBar::new("gallery-compare-bar", bar()))
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .gap(t.spacing.md)
                .child(pane(0, "gallery-compare-left"))
                .child(pane(1, "gallery-compare-right")),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .h(px(380.))
                .gap(t.spacing.md)
                .child(stream_column(&t, Layout::SideBySide))
                .child(stream_column(&t, Layout::Unified)),
        )
        .child(gutter_sample())
}

/// A card line from line numbers and texts.
pub(crate) fn card_line(row: DiffRow, hunk: u32, left: &str, right: &str) -> CardRow {
    let text = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    CardRow::Line {
        row,
        hunk,
        left: text(left),
        right: text(right),
    }
}

/// A card of `kind` with `rows`.
pub(crate) fn sample_card(kind: PairKind, rows: Vec<CardRow>, viewed: bool) -> Card {
    Card {
        kind,
        rows,
        hunks: Vec::new(),
        added: 3,
        removed: 2,
        viewed_key: ViewedKey([0; 64]),
        viewed,
        viewable: true,
    }
}

/// A Stream model with an open text card, a binary summary, a pending card, a viewed card
/// and a rules group.
fn stream_model(layout: Layout) -> StreamModel {
    let mut model = StreamModel::default();
    model.set_items(1, stream_items(&stream_rows_fake(), ""));
    let cards = [
        (1, sample_card(PairKind::Text, text_rows(), false)),
        (2, sample_card(PairKind::Binary, Vec::new(), false)),
        (4, sample_card(PairKind::Text, Vec::new(), true)),
    ];
    for (id, card) in cards {
        let update = StreamUpdate::Card {
            generation: 1,
            id,
            card: Arc::new(card),
        };
        model.apply(&update);
    }
    if layout == Layout::SideBySide {
        model.next_change(1);
    }
    model
}

/// The compare rows the fake Stream lists.
fn stream_rows_fake() -> [CompareRow; 6] {
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    });
    let row =
        |id: u64, path: &str, status| CompareRow::new(id, path.to_owned(), status, (side, side));
    [
        row(1, "crates/core/src/walk.rs", RowStatus::Different),
        row(2, "assets/logo.bin", RowStatus::Different),
        row(3, "crates/core/src/lib.rs", RowStatus::Checking),
        row(4, "docs/notes.md", RowStatus::Different),
        row(5, "a.txt", RowStatus::SameAfterRules),
        row(6, "b.txt", RowStatus::SameAfterRules),
    ]
}

/// The rows of the open text card: lines, a gap and a "more" note.
fn text_rows() -> Vec<CardRow> {
    vec![
        card_line(
            DiffRow::Equal(10, 10),
            0,
            "fn walk(root: &Path) {",
            "fn walk(root: &Path) {",
        ),
        card_line(
            DiffRow::Changed(11, 11),
            0,
            "    let mut seen = 0;",
            "    let mut seen = 0u64;",
        ),
        card_line(DiffRow::Deleted(12), 0, "    // old counter", ""),
        card_line(DiffRow::Inserted(12), 0, "", "    // counts every entry"),
        card_line(DiffRow::Equal(13, 13), 0, "}", "}"),
        CardRow::Gap { lines: 42 },
        card_line(DiffRow::Equal(56, 56), 1, "fn done() {}", "fn done() {}"),
        card_line(DiffRow::Inserted(57), 1, "", "fn extra() {}"),
        CardRow::More { hunks: 3 },
    ]
}

/// One Stream column: every card row kind in `layout`, inside a bordered panel.
fn stream_column(t: &tokens::Theme, layout: Layout) -> Div {
    let model = stream_model(layout);
    let rows = stream_rows(&model, layout);
    let ctx = CardCtx {
        t,
        model: &model,
        layout,
        bars: None,
    };
    let views: Vec<AnyElement> = (0..rows.len())
        .map(|at| stream_row_view(&rows, at, &ctx))
        .collect();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .p(t.spacing.md)
        .rounded(t.radii.card)
        .bg(t.colors.background)
        .children(views)
}

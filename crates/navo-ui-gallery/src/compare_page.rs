use std::sync::Arc;

use navo_ui::browser::{BarView, CompareFilter, CompareKind, RowView, SideView, Tone, Total};
use navo_ui::compare_list::{CompareBar, CompareList};
use navo_ui::prelude::*;
use navo_ui::primitives::ListScroll;
use navo_ui::tokens;

struct Sample {
    name: &'static str,
    kind: CompareKind,
    sides: [Option<(&'static str, &'static str)>; 2],
    tone: Tone,
    mark: &'static str,
    status: &'static str,
}

const TIME: &str = "2026-10-20 13:41";

const SAMPLES: [Sample; 10] = [
    sample(
        "docs",
        CompareKind::Folder,
        [Some(("<DIR>", TIME)); 2],
        (Tone::Muted, "="),
    ),
    sample(
        "Cargo.toml",
        CompareKind::File,
        [Some(("1.6 KB", TIME)), Some(("1.7 KB", TIME))],
        (Tone::Change, "≠"),
    )
    .said("different"),
    sample(
        "legacy_walk.rs",
        CompareKind::File,
        [Some(("5.3 KB", TIME)), None],
        (Tone::Delete, "←"),
    )
    .said("only left"),
    sample(
        "write_file.rs",
        CompareKind::File,
        [None, Some(("6.7 KB", TIME))],
        (Tone::Add, "→"),
    )
    .said("only right"),
    sample(
        "big_trace.asc",
        CompareKind::File,
        [Some(("612 MB", TIME)), Some(("612 MB", TIME))],
        (Tone::Busy, "…"),
    )
    .said("same size and time · checking · 208.0 MB of 612.0 MB"),
    sample(
        "flash_dump.hex",
        CompareKind::File,
        [Some(("148 MB", TIME)), Some(("148 MB", TIME))],
        (Tone::Muted, "–"),
    )
    .said("Not checked (stopped)"),
    sample(
        "notes",
        CompareKind::Folder,
        [Some(("<DIR>", TIME)), Some(("1.0 KB", TIME))],
        (Tone::Conflict, "!"),
    )
    .said("type conflict"),
    sample(
        "secret.key",
        CompareKind::File,
        [Some(("64 B", TIME)), Some(("64 B", TIME))],
        (Tone::Conflict, "!"),
    )
    .said("unavailable: access denied"),
    sample(
        "README.md",
        CompareKind::File,
        [Some(("5.9 KB", TIME)), Some(("5.9 KB", TIME))],
        (Tone::Muted, "="),
    )
    .said("same after rules"),
    sample(
        "lib.rs",
        CompareKind::File,
        [Some(("2.0 KB", TIME)), Some(("2.0 KB", TIME))],
        (Tone::Muted, "="),
    ),
];

const fn sample(
    name: &'static str,
    kind: CompareKind,
    sides: [Option<(&'static str, &'static str)>; 2],
    (tone, mark): (Tone, &'static str),
) -> Sample {
    Sample {
        name,
        kind,
        sides,
        tone,
        mark,
        status: "",
    }
}

impl Sample {
    const fn said(mut self, status: &'static str) -> Self {
        self.status = status;
        self
    }

    fn view(&self, side: usize) -> RowView {
        let here = self.sides[side].map(|(size, time)| SideView {
            kind: self.kind,
            size: size.to_owned(),
            time: time.to_owned(),
        });
        RowView {
            path: self.name.to_owned(),
            name: self.name.to_owned(),
            status: if here.is_some() {
                self.status.to_owned()
            } else {
                String::new()
            },
            side: here,
            tone: self.tone,
            mark: self.mark,
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
}

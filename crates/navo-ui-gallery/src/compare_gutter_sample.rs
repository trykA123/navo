//! The folder compare's centre gutter on the gallery: every label, on both themes, at both widths.
//!
//! Each panel builds its own `Theme`, so the light and dark looks sit side by side.

use navo_ui::browser::{CompareKind, CompareRow, CompareSide, RowProgress, RowStatus};
use navo_ui::compare_list::{GutterLabel, gutter_cell, gutter_label, gutter_width};
use navo_ui::prelude::*;
use navo_ui::tokens::{Mode, Theme};

const FILE: CompareSide = CompareSide {
    kind: CompareKind::File,
    size: 1,
    modified_ms: None,
};
const DIR: CompareSide = CompareSide {
    kind: CompareKind::Folder,
    ..FILE
};

/// A sample row: a name and the label the gutter draws for it.
struct Entry {
    name: &'static str,
    label: Option<GutterLabel>,
}

fn entry(name: &'static str, row: &CompareRow, progress: Option<RowProgress>) -> Entry {
    Entry {
        name,
        label: gutter_label(row, progress),
    }
}

fn file(status: RowStatus) -> CompareRow {
    CompareRow::new(1, "a", status, (Some(FILE), Some(FILE)))
}

fn folder(status: RowStatus, inside: u32) -> CompareRow {
    let mut row = CompareRow::new(2, "d", status, (Some(DIR), Some(DIR)));
    row.contains_differences = inside > 0;
    row.differences_inside = inside;
    row
}

/// One row per label, plus the identical row that has none.
fn entries() -> Vec<Entry> {
    let checking = RowProgress {
        id: 1,
        bytes_done: 410,
        bytes_total: 1000,
    };
    vec![
        entry("same", &folder(RowStatus::Same, 0), None),
        entry("changed", &file(RowStatus::Different), None),
        entry("only left", &file(RowStatus::OnlyLeft), None),
        entry("only right", &file(RowStatus::OnlyRight), None),
        entry("type conflict", &file(RowStatus::TypeConflict), None),
        entry("inside (same folder)", &folder(RowStatus::Same, 12), None),
        entry(
            "inside (changed folder)",
            &folder(RowStatus::Different, 3),
            None,
        ),
        entry("same by rules", &file(RowStatus::SameAfterRules), None),
        entry("checking", &file(RowStatus::Checking), Some(checking)),
        entry("pending", &file(RowStatus::Checking), None),
        entry(
            "unchecked",
            &file(RowStatus::NotChecked("compare-reason-stopped")),
            None,
        ),
        entry(
            "can't read",
            &file(RowStatus::Unavailable("compare-reason-access-denied")),
            None,
        ),
    ]
}

fn cell_row(t: &Theme, id: (usize, usize), entry: &Entry, width: Pixels) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(28.))
        .child(
            div()
                .flex_1()
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(entry.name),
        )
        .child(gutter_cell(
            t,
            ("gutter-sample", id.0 * 100 + id.1),
            width,
            entry.label.as_ref(),
        ))
}

fn panel(at: usize, mode: Mode, width: Pixels) -> Div {
    let t = Theme::new(mode);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .bg(t.surfaces.card)
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .text_color(t.colors.text)
        .children(
            entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| cell_row(&t, (at, index), entry, width)),
        )
}

/// The four panels: light and dark, at the wide and the narrow gutter width.
pub fn gutter_sample() -> Div {
    let widths = [gutter_width(px(1440.)), gutter_width(px(390.))];
    let panels = widths
        .into_iter()
        .flat_map(|width| [Mode::Light, Mode::Dark].map(move |mode| (mode, width)));
    div().flex().flex_none().gap(px(12.)).children(
        panels
            .enumerate()
            .map(|(at, (mode, width))| panel(at, mode, width)),
    )
}

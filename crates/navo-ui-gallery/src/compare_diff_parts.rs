//! The chrome around the compare diff samples: header, status line, the two file drawers and
//! the Stream outline drawer. It mirrors the app's layout with the same tokens and sizes.

use navo_ui::browser::RowStatus;
use navo_ui::compare_diff::{DiffRow, PairText, StreamItem, StreamModel};
use navo_ui::prelude::*;
use navo_ui::primitives::{IconName, icon};
use navo_ui::tokens::Theme;

/// Width of a file drawer, in px (16 rem).
const DRAWER: f32 = 256.;
/// Width of the outline drawer, in px.
const OUTLINE: f32 = 248.;
/// Height of an outline line, in px.
const LINE: f32 = 28.;
/// Size of the icons in an outline line, in px.
const ICON: f32 = 14.;

/// Small muted text.
fn small(t: &Theme) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

/// The header: back arrow, file name, folder and sides, encoding.
pub fn header(t: &Theme, name: &str, meta: &str, encoding: &str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(icon(IconName::ArrowLeft, px(ICON), t.colors.muted))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(name.to_owned()),
        )
        .child(small(t).child(meta.to_owned()))
        .child(small(t).flex_none().child(encoding.to_owned()))
}

/// The status line: the counter (with a progress bar when `share` is set), then the way back.
pub fn status_line(t: &Theme, counter: &str, share: Option<f32>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.xl)
        .h(t.spacing.path_height)
        .border_t_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .when_some(share, |line, share| {
            line.child(
                div()
                    .flex_none()
                    .w(px(64.))
                    .h(px(4.))
                    .rounded(px(2.))
                    .overflow_hidden()
                    .bg(t.surfaces.progress_track)
                    .child(
                        div()
                            .h_full()
                            .w(relative(share))
                            .bg(t.surfaces.progress_fill),
                    ),
            )
        })
        .child(small(t).child(counter.to_owned()))
        .child(div().flex_1())
        .child(small(t).child("Esc goes back."))
}

/// A drawer frame with a title.
fn drawer(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(DRAWER))
        .min_h_0()
        .bg(t.colors.rail)
        .border_color(t.colors.line)
}

/// The drawer title row.
fn title(t: &Theme, text: &str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .px(t.spacing.md)
        .h(t.spacing.row_height)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(t.typography.small)
        .child(text.to_owned())
}

/// The list of changes with the row `cursor` picked.
pub fn changes(t: &Theme, text: &PairText, cursor: usize) -> Div {
    let rows = text.hunks.iter().enumerate().map(|(at, hunk)| {
        let row = text.rows[hunk.rows.start as usize];
        let line = match row {
            DiffRow::Equal(l, _) | DiffRow::Changed(l, _) | DiffRow::Deleted(l) => l,
            DiffRow::Inserted(r) => r,
        };
        div()
            .flex()
            .items_center()
            .h(t.spacing.row_height)
            .px(t.spacing.md)
            .text_size(t.typography.small)
            .when(at == cursor, |d| d.bg(t.surfaces.current))
            .child(format!(
                "Line {}  +{} −{}",
                line + 1,
                hunk.added,
                hunk.removed
            ))
    });
    drawer(t)
        .border_r_1()
        .child(title(t, "Changes"))
        .children(rows)
}

/// One muted line of the details drawer.
fn fact(t: &Theme, text: &str) -> Div {
    small(t)
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .child(text.to_owned())
}

/// The details drawer: encoding, line endings and whether the bytes differ.
pub fn details(t: &Theme) -> Div {
    drawer(t)
        .border_l_1()
        .child(title(t, "Details"))
        .child(fact(t, "Encoding: UTF-8"))
        .child(fact(t, "Line endings: LF ↔ CRLF"))
}

/// The glyph, text colour and badge wash of a status in the outline.
fn badge(t: &Theme, status: RowStatus) -> Div {
    let (mark, text, wash) = match status {
        RowStatus::OnlyLeft => ("←", t.diff.delete.text, Some(t.diff.delete.wash)),
        RowStatus::Checking => ("…", t.colors.accent_text, None),
        _ => ("≠", t.diff.change.text, Some(t.diff.change.wash)),
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(18.))
        .rounded(t.radii.xs)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .font_weight(FontWeight::BOLD)
        .text_color(text)
        .when_some(wash, Styled::bg)
        .child(mark)
}

/// A folder heading line of the outline.
fn folder_line(t: &Theme, dir: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(LINE))
        .px(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(icon(IconName::Folder, px(ICON), t.colors.folder))
        .child(div().min_w_0().truncate().child(dir.to_owned()))
}

/// One file line of the outline; `in_view` adds the accent bar, `viewed` the tick.
fn file_line(t: &Theme, item: &StreamItem, (in_view, viewed): (bool, bool)) -> Div {
    let name = item.path.rsplit_once('/').map_or(&*item.path, |(_, n)| n);
    div()
        .relative()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(LINE))
        .pl(px(22.))
        .pr(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(if viewed {
            t.colors.muted
        } else {
            t.colors.text
        })
        .when(in_view, |d| {
            d.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(t.colors.accent),
            )
        })
        .child(badge(t, item.status))
        .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
        .when(viewed, |d| {
            d.child(icon(IconName::Check, px(ICON), t.colors.muted))
        })
}

/// The outline drawer: files grouped by folder, the file in view marked, Viewed ticks.
/// `narrow` lays it over the cards like the app does below 900 px.
pub fn outline(t: &Theme, model: &StreamModel, in_view: usize, narrow: bool) -> Div {
    let mut lines: Vec<Div> = Vec::new();
    let mut last: Option<&str> = None;
    for (at, item) in model.items().iter().enumerate() {
        let dir = item.path.rsplit_once('/').map_or("", |(d, _)| d);
        if last != Some(dir) {
            last = Some(dir);
            lines.push(folder_line(t, dir));
        }
        let state = (at == in_view, model.is_viewed(item.id));
        lines.push(file_line(t, item, state));
    }
    lines.push(
        div()
            .flex()
            .items_center()
            .h(px(LINE))
            .px(t.spacing.md)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(format!("{} files same after rules", model.rules())),
    );
    let frame = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(OUTLINE))
        .py(t.spacing.md)
        .border_r_1()
        .border_color(t.colors.line);
    match narrow {
        true => frame
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .bg(t.colors.surface)
            .shadow(t.shadows.menu.clone())
            .occlude(),
        false => frame.bg(t.colors.rail),
    }
    .children(lines)
}

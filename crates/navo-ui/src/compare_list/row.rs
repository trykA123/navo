use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, ParentElement, Pixels, SharedString, Styled, div, pattern_slash, px,
};
use navo_fs::EntryKind;

use super::tone;
use crate::browser::{CompareKind, RowView, SideView};
use crate::file_list::kind;
use crate::file_list::row::{cursor_bar, inset_ring};
use crate::primitives::icon;
use crate::tokens::Theme;

const STATUS_MAX: Pixels = px(340.);
const NAME_MIN: Pixels = px(110.);
const SIZE_WIDTH: Pixels = px(72.);
const DATE_WIDTH: Pixels = px(124.);

pub(super) struct RowLook<'a> {
    pub theme: &'a Theme,
    pub row: &'a RowView,
    pub is_selected: bool,
    pub is_active: bool,
}

fn frame(t: &Theme) -> Div {
    div()
        .relative()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .mx(t.spacing.xs)
        .rounded(t.radii.md)
}

/// A row with no file on this side: the words that say so on a hatch, so the other side's row
/// stays lined up. The hatch is the file diff's.
fn gap(look: &RowLook) -> Div {
    let t = look.theme;
    let selected = look.is_selected;
    frame(t)
        .bg(pattern_slash(t.diff.gap_hatch, 1., 4.))
        .pl(t.rows.inset)
        .when(selected, |d| d.shadow(inset_ring(t.colors.line)))
        .when(selected && look.is_active, |d| d.child(cursor_bar(t)))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(SharedString::from(look.row.status.clone())),
        )
}

fn entry_kind(kind: CompareKind) -> EntryKind {
    match kind {
        CompareKind::File => EntryKind::File,
        CompareKind::Link => EntryKind::Link,
        CompareKind::Folder | CompareKind::Submodule => EntryKind::Folder,
    }
}

fn name_text(row: &RowView, side: &SideView) -> String {
    match side.kind {
        CompareKind::Folder | CompareKind::Submodule => format!("[{}]", row.name),
        CompareKind::File | CompareKind::Link => row.name.clone(),
    }
}

/// The mark that sits before the date of the newer side of a pair.
const NEWER_MARK: &str = "▲";

/// A right-aligned, truncated number cell `width` wide. `mark` sits inside the same width, so a
/// marked date loses room instead of pushing the column.
fn number(width: Pixels, mark: Option<&str>, text: &str) -> Div {
    div()
        .w(width)
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .overflow_hidden()
        .children(mark.map(|mark| div().flex_none().mr(px(2.)).child(SharedString::from(mark))))
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(SharedString::from(text.to_owned())),
        )
}

/// The size and the date of one side: loud and marked on the newer side, muted on the other.
fn numbers(t: &Theme, side: &SideView) -> Div {
    let (colour, weight) = if side.newer {
        (t.colors.text, FontWeight::SEMIBOLD)
    } else {
        (t.colors.muted, FontWeight::NORMAL)
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .flex_none()
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .text_size(t.typography.small)
        .text_color(colour)
        .font_weight(weight)
        .child(number(SIZE_WIDTH, None, &side.size))
        .child(number(
            DATE_WIDTH,
            side.newer.then_some(NEWER_MARK),
            &side.time,
        ))
}

fn name_color(t: &Theme, row: &RowView) -> gpui_kit::Hsla {
    match row.tone {
        crate::browser::Tone::Muted => t.colors.muted,
        _ => tone::text(t, row.tone),
    }
}

/// One pane's row: icon, name, status words, size and time. The gutter column comes from `list`.
pub(super) fn row_element(look: &RowLook) -> Div {
    let Some(side) = &look.row.side else {
        return gap(look);
    };
    let theme = look
        .theme
        .selected_content(look.is_selected && look.is_active);
    let t: &Theme = &theme;
    let row = look.row;
    let (_, extension) = kind::split_name(&row.name, entry_kind(side.kind));
    let (glyph, glyph_color) = kind::icon(entry_kind(side.kind), extension, &t.colors);
    let loud = !matches!(row.tone, crate::browser::Tone::Muted);
    frame(t)
        .gap(t.spacing.md)
        .pl(t.rows.inset)
        .pr(t.spacing.xxl)
        .when(look.is_selected && look.is_active, |d| {
            d.bg(t.surfaces.current).child(cursor_bar(t))
        })
        .when(look.is_selected && !look.is_active, |d| {
            d.shadow(inset_ring(t.colors.line))
        })
        .child(icon(glyph, t.spacing.icon - px(1.), glyph_color))
        .child(
            div()
                .flex_1()
                .min_w(NAME_MIN)
                .truncate()
                .text_color(name_color(t, row))
                .when(loud, |d| d.font_weight(FontWeight::SEMIBOLD))
                .child(name_text(row, side)),
        )
        .when(!row.status.is_empty(), |d| {
            d.child(
                div()
                    .min_w_0()
                    .max_w(STATUS_MAX)
                    .truncate()
                    .text_size(t.typography.small)
                    .text_color(tone::text(t, row.tone))
                    .child(SharedString::from(row.status.clone())),
            )
        })
        .child(numbers(t, side))
}

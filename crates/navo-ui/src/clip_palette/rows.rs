use std::ops::Range;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, HighlightStyle, Hsla, InteractiveElement, ParentElement, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, StyledText, div, px,
};
use navo_l10n::tr;

use super::age::age;
use super::{ClipKind, ClipRow, Origin};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

pub(super) const ROW: f32 = 28.;
pub(super) const MASK: &str = "••••••••••";

pub(super) type RowElement = ObservedElement<Stateful<Div>>;

/// What a row needs besides its data: where it is and what the clock says.
#[derive(Clone, Copy)]
pub(super) struct Place {
    pub index: usize,
    pub is_selected: bool,
    pub now_ms: i64,
    pub gutter: Pixels,
    pub shows_source: bool,
}

pub(super) fn kind_icon(t: &Theme, kind: ClipKind) -> (IconName, Hsla) {
    match kind {
        ClipKind::Text => (IconName::FileText, t.colors.muted),
        ClipKind::Link => (IconName::Link, t.colors.accent_text),
        ClipKind::Path => (IconName::Folder, t.colors.folder),
        ClipKind::Files => (IconName::Files, t.colors.folder),
        ClipKind::Image => (IconName::FileImage, t.colors.kind_image),
        ClipKind::Secret => (IconName::Lock, t.colors.warn),
    }
}

pub(super) fn is_mono(kind: ClipKind) -> bool {
    matches!(kind, ClipKind::Path | ClipKind::Files)
}

/// Byte ranges that fit the text, in order and apart, so a bad range never panics.
fn valid_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut sorted: Vec<Range<usize>> = ranges
        .iter()
        .filter(|range| {
            range.start < range.end
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end)
        })
        .cloned()
        .collect();
    sorted.sort_by_key(|range| range.start);
    let mut apart: Vec<Range<usize>> = Vec::new();
    for range in sorted {
        if apart.last().is_none_or(|last| last.end <= range.start) {
            apart.push(range);
        }
    }
    apart
}

fn marked(t: &Theme, text: &str, ranges: &[Range<usize>]) -> StyledText {
    let found = HighlightStyle {
        color: Some(t.colors.accent_text),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..HighlightStyle::default()
    };
    let runs: Vec<_> = valid_ranges(text, ranges)
        .into_iter()
        .map(|range| (range, found))
        .collect();
    StyledText::new(SharedString::from(text.to_owned())).with_highlights(runs)
}

fn base(index: usize) -> RowElement {
    div()
        .id(("clip-palette-row", index))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .w_full()
        .h(px(ROW))
}

/// A group head in All: "Recent clipboard" or "Saved".
pub(super) fn head(t: &Theme, origin: Origin, index: usize) -> RowElement {
    let label: SharedString = match origin {
        Origin::Clipboard => tr("clip-head-recent"),
        Origin::Saved => tr("clip-head-saved"),
    }
    .into();
    base(index)
        .role(Role::Heading)
        .aria_label(label.clone())
        .items_end()
        .px(t.spacing.md)
        .pb(t.spacing.sm)
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.muted)
        .child(div().min_w_0().truncate().child(label))
}

fn title(t: &Theme, row: &ClipRow) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when(
            is_mono(row.kind) && row.from == Origin::Clipboard,
            |title| {
                title
                    .font_family(t.typography.mono_font.clone())
                    .text_size(t.typography.small)
            },
        )
        .child(marked(t, &row.title, &row.ranges))
}

fn meta(t: &Theme, row: &ClipRow, place: Place) -> Div {
    let source = row
        .source_app
        .clone()
        .filter(|_| place.shows_source && row.from == Origin::Clipboard);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .whitespace_nowrap()
        .children(source.map(SharedString::from))
        .child(age(place.now_ms, row.when_ms))
}

/// One option: kind icon, title or value, and the age.
pub(super) fn row(theme: &Theme, row: &ClipRow, place: Place) -> RowElement {
    let theme = theme.selected_content(place.is_selected);
    let t = &*theme;
    let (name, color) = kind_icon(t, row.kind);
    base(place.index)
        .role(Role::ListBoxOption)
        .aria_selected(place.is_selected)
        .aria_label(SharedString::from(row.title.clone()))
        .gap(t.spacing.md)
        .pl(t.spacing.md)
        .pr(t.spacing.md + place.gutter)
        .rounded(t.radii.md)
        .cursor_pointer()
        .when(place.is_selected, |option| {
            option
                .bg(t.colors.selection)
                .text_color(t.colors.on_selection)
        })
        .child(icon(name, t.spacing.icon, color))
        .child(title(t, row))
        .child(meta(t, row, place))
}

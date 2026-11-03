use std::ops::Range;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, HighlightStyle, Hsla, InteractiveElement, ParentElement, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, StyledText, div, px,
};
use navo_l10n::tr;

use super::age::age;
use super::state::NotesHead;
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
    /// A note whose folder is gone: the row says so in place of its age.
    pub is_missing: bool,
    /// The row's index in the results, when the list shows Alt+digit numbers.
    pub slot: Option<usize>,
}

/// The Alt+digit shortcut of the results row at `slot`: 1 to 9, then none.
pub(super) fn digit(slot: usize) -> Option<char> {
    u32::try_from(slot + 1)
        .ok()
        .filter(|n| (1..=9).contains(n))
        .and_then(|n| char::from_digit(n, 10))
}

/// A fixed-width cell so titles line up whether or not the row has a number.
fn number(t: &Theme, slot: usize) -> Div {
    div()
        .flex_none()
        .w(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(digit(slot).map(String::from).unwrap_or_default())
}

pub(super) fn kind_icon(t: &Theme, kind: ClipKind) -> (IconName, Hsla) {
    match kind {
        ClipKind::Text => (IconName::FileText, t.colors.muted),
        ClipKind::Link => (IconName::Link, t.colors.accent_text),
        ClipKind::Path => (IconName::Folder, t.colors.folder),
        ClipKind::Files => (IconName::Files, t.colors.folder),
        ClipKind::Image => (IconName::FileImage, t.colors.kind_image),
        ClipKind::Secret => (IconName::Lock, t.colors.warn),
        ClipKind::Note => (IconName::FilePen, t.colors.muted),
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
    head_line(
        t,
        tr(match origin {
            Origin::Clipboard => "clip-head-recent",
            Origin::Saved => "clip-head-saved",
        }),
        index,
    )
}

fn head_line(t: &Theme, label: String, index: usize) -> RowElement {
    let label = SharedString::from(label);
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

/// A group head of Notes mode.
pub(super) fn notes_head(t: &Theme, which: NotesHead, index: usize) -> RowElement {
    head_line(
        t,
        tr(match which {
            NotesHead::ThisFolder => "clip-head-this-folder",
            NotesHead::Others => "clip-head-other-notes",
        }),
        index,
    )
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

fn folder_missing(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .child(icon(
            IconName::TriangleAlert,
            t.typography.small,
            t.colors.warn,
        ))
        .child(tr("clip-note-folder-missing"))
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
        .children(if place.is_missing {
            Some(folder_missing(t))
        } else {
            (row.when_ms > 0).then(|| div().child(age(place.now_ms, row.when_ms)))
        })
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
        .children(place.slot.map(|slot| number(t, slot)))
        .child(icon(name, t.spacing.icon, color))
        .child(title(t, row))
        .child(meta(t, row, place))
}

#[cfg(test)]
mod tests {
    use super::digit;

    #[test]
    fn only_the_first_nine_rows_have_a_digit() {
        let shown: Vec<Option<char>> = (0..11).map(digit).collect();
        assert_eq!(shown[0], Some('1'));
        assert_eq!(shown[8], Some('9'));
        assert_eq!(shown[9], None);
        assert_eq!(shown[10], None);
    }
}

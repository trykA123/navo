use std::ops::Range;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{HighlightStyle, Stateful, StyledText};
use navo_core::paging::{HEX_WIDTH, TextLayout};

use super::lines::{HexLine, TextLine};
use super::spans::{Paint, Painted, TextCut, hex_slots, line_end, shown_text};
use crate::prelude::*;
use crate::tokens::Theme;

const SHOWN_CHARS: usize = 1000;
const MATCH_OPACITY: f32 = 0.3;

pub(crate) struct RowLook<'a> {
    pub t: &'a Theme,
    pub height: Pixels,
    pub painted: Painted<'a>,
    pub bookmarks: &'a [u64],
    pub line_ends: bool,
}

pub(super) struct TextLook {
    pub digits: usize,
    pub shift: usize,
    pub numbers: bool,
    pub layout: TextLayout,
}

impl RowLook<'_> {
    fn style(&self, paint: Paint) -> HighlightStyle {
        let (background, color) = match paint {
            Paint::Hit => (self.t.colors.accent, Some(self.t.colors.background)),
            Paint::Mark(colour) => (
                self.t.marks.fills[usize::from(colour) % self.t.marks.fills.len()],
                Some(self.t.colors.text),
            ),
            Paint::Match => (self.t.colors.accent.opacity(MATCH_OPACITY), None),
        };
        HighlightStyle {
            background_color: Some(background),
            color,
            ..HighlightStyle::default()
        }
    }

    fn styled(&self, runs: Vec<(Range<usize>, Paint)>) -> Vec<(Range<usize>, HighlightStyle)> {
        runs.into_iter()
            .map(|(range, paint)| (range, self.style(paint)))
            .collect()
    }

    fn is_bookmarked(&self, span: Range<u64>) -> bool {
        let first = self.bookmarks.partition_point(|at| *at < span.start);
        self.bookmarks.get(first).is_some_and(|at| *at < span.end)
    }

    fn gutter(&self, index: usize, is_bookmarked: bool) -> impl IntoElement {
        let t = self.t;
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(t.spacing.sm)
            .when(is_bookmarked, |slot| {
                slot.child(
                    div()
                        .id(("paged-bookmark", index))
                        .aria_label("Bookmark")
                        .test_support()
                        .size(t.spacing.sm)
                        .rounded_full()
                        .bg(t.colors.muted),
                )
            })
    }
}

fn row_frame(look: &RowLook<'_>, index: usize, label: String) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("paged-row", index))
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(look.t.spacing.xl)
        .h(look.height)
        .pl(look.t.spacing.xs)
        .pr(look.t.spacing.xl)
        .whitespace_nowrap()
        .overflow_hidden()
}

type Runs = Vec<(Range<usize>, Paint)>;

fn hex_ranges(slots: &[(usize, Paint)]) -> (Runs, Runs) {
    slots
        .iter()
        .map(|(slot, paint)| {
            let at = slot * 3 + usize::from(*slot >= 8);
            ((at..at + 2, *paint), (*slot..slot + 1, *paint))
        })
        .unzip()
}

pub(crate) fn hex_row(look: &RowLook<'_>, index: usize, line: &HexLine) -> AnyElement {
    let (hex_paint, ascii_paint) = hex_ranges(&hex_slots(line.start, &look.painted));
    let is_bookmarked = look.is_bookmarked(line.start..line.start + HEX_WIDTH);
    let styles = (look.styled(hex_paint), look.styled(ascii_paint));
    hex_body(look, (index, line, is_bookmarked), styles)
}

/// One hex row of the compare diff: the shared frame and aria label, tinted with the diff
/// tokens for the changed byte slots and a stronger tint for the current region.
pub(crate) fn hex_row_tinted(
    t: &Theme,
    index: usize,
    line: &HexLine,
    tints: &[(usize, bool)],
) -> AnyElement {
    let look = RowLook {
        t,
        height: t.spacing.row_height,
        painted: Painted {
            hit: None,
            marks: &[],
            matches: &[],
        },
        bookmarks: &[],
        line_ends: false,
    };
    hex_body(&look, (index, line, false), tint_styles(t, tints))
}

/// The one body both hex rows draw, at the look's height: gutter, offset, hex and ASCII.
fn hex_body(
    look: &RowLook<'_>,
    (index, line, is_bookmarked): (usize, &HexLine, bool),
    (hex_styles, ascii_styles): (Styles, Styles),
) -> AnyElement {
    let row = &line.row;
    let label = format!("{}  {}  {}", row.offset, row.hex, row.ascii);
    row_frame(look, index, label)
        .child(look.gutter(index, is_bookmarked))
        .child(
            div()
                .text_color(look.t.colors.muted)
                .child(row.offset.clone()),
        )
        .child(StyledText::new(row.hex.clone()).with_highlights(hex_styles))
        .child(
            div()
                .text_color(look.t.colors.muted)
                .child(StyledText::new(row.ascii.clone()).with_highlights(ascii_styles)),
        )
        .into_any_element()
}

/// The highlight ranges of tinted slots in the hex text, and in the ASCII text.
type Styles = Vec<(Range<usize>, HighlightStyle)>;

fn tint_styles(t: &Theme, slots: &[(usize, bool)]) -> (Styles, Styles) {
    let mut hex = Vec::with_capacity(slots.len());
    let mut ascii = Vec::with_capacity(slots.len());
    for &(slot, current) in slots {
        let colour = match current {
            true => t.diff.change.mark,
            false => t.diff.change.fill,
        };
        let style = HighlightStyle {
            background_color: Some(colour),
            ..HighlightStyle::default()
        };
        let at = slot * 3 + usize::from(slot >= 8);
        hex.push((at..at + 2, style));
        ascii.push((slot..slot + 1, style));
    }
    (hex, ascii)
}

fn line_end_glyph(
    look: &RowLook<'_>,
    unit: usize,
    index: usize,
    line: &TextLine,
) -> Option<impl IntoElement> {
    let glyph =
        line_end(line.start, line.end, line.next, unit as u64).filter(|_| look.line_ends)?;
    Some(
        div()
            .id(("paged-eol", index))
            .aria_label(glyph)
            .test_support()
            .flex_none()
            .px(look.t.spacing.xxs)
            .rounded(look.t.radii.xs)
            .border_1()
            .border_color(look.t.colors.line)
            .text_size(look.t.typography.tiny)
            .text_color(look.t.colors.muted)
            .child(glyph),
    )
}

pub(super) fn text_row(
    look: &RowLook<'_>,
    text: &TextLook,
    index: usize,
    line: &TextLine,
) -> AnyElement {
    let number = line
        .line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let cut = TextCut {
        start: line.text_from,
        end: line.end,
        shift: text.shift,
        limit: SHOWN_CHARS,
        layout: text.layout,
    };
    let (shown, runs) = shown_text(&line.text, &cut, &look.painted);
    let span = line.start..line.next.max(line.start + 1);
    let is_hit = look
        .painted
        .hit
        .is_some_and(|hit| span.contains(&hit.start));
    let runs = look.styled(runs);
    let theme = look.t.selected_content(is_hit);
    let look = &RowLook {
        t: &theme,
        painted: Painted { ..look.painted },
        ..*look
    };
    row_frame(look, index, line.text.clone())
        .when(look.t.is_contrast, |row| row.text_color(look.t.colors.text))
        .when(is_hit, |row| row.bg(look.t.colors.selection))
        .child(look.gutter(index, look.is_bookmarked(span)))
        .when(text.numbers, |row| {
            row.child(
                div()
                    .id(("paged-line", index))
                    .aria_label(number.clone())
                    .test_support()
                    .flex_none()
                    .text_color(look.t.colors.muted)
                    .child(format!("{number:>width$}", width = text.digits)),
            )
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(look.t.spacing.xs)
                .child(StyledText::new(shown).with_highlights(runs))
                .children(line_end_glyph(look, text.layout.unit(), index, line)),
        )
        .into_any_element()
}

pub(super) fn missing_row(look: &RowLook<'_>, index: usize) -> AnyElement {
    row_frame(look, index, String::new())
        .text_color(look.t.colors.muted)
        .into_any_element()
}

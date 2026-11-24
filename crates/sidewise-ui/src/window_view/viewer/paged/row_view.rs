use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::{HighlightStyle, InteractiveText, Stateful, StyledText, UnderlineStyle};
use navo_core::paging::{HEX_WIDTH, TextLayout};

use super::lines::{HexLine, TextLine};
use super::spans::{Paint, Painted, TextCut, hex_slots, line_end, shown_text};
use super::syntax_runs::{overlay, syntax_runs};
use crate::prelude::*;
use crate::tokens::Theme;

const SHOWN_CHARS: usize = 1000;
const MATCH_OPACITY: f32 = 0.3;
const CURSOR_UNDERLINE: f32 = 1.;

/// What a click on a hex or ASCII byte does: it gets the byte's file offset.
pub type PickByte = Rc<dyn Fn(u64, &mut Window, &mut App)>;

/// The hex cursor a viewer row draws: the byte it sits on and what a click on a byte does.
#[doc(hidden)]
pub struct HexCursor {
    pub at: Option<u64>,
    pub pick: Option<PickByte>,
}

#[doc(hidden)]
pub struct RowLook<'a> {
    pub t: &'a Theme,
    pub height: Pixels,
    pub painted: Painted<'a>,
    pub bookmarks: &'a [u64],
    pub line_ends: bool,
}

#[doc(hidden)]
pub struct TextLook {
    pub digits: usize,
    pub shift: usize,
    pub numbers: bool,
    pub layout: TextLayout,
    pub syntax: Option<Arc<HighlightTheme>>,
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
        .map(|(slot, paint)| ((hex_range(*slot), *paint), (*slot..slot + 1, *paint)))
        .unzip()
}

/// The cursor look: the selection fill with an accent underline, so it is not colour alone.
fn cursor_style(t: &Theme) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(t.colors.selection),
        color: Some(t.colors.text),
        underline: Some(UnderlineStyle {
            thickness: px(CURSOR_UNDERLINE),
            color: Some(t.colors.accent),
            wavy: false,
        }),
        ..HighlightStyle::default()
    }
}

/// Puts the cursor style on `range`, replacing any paint it overlaps; keeps the runs sorted.
fn with_cursor(mut styles: Styles, range: Range<usize>, style: HighlightStyle) -> Styles {
    styles.retain(|(run, _)| run.end <= range.start || run.start >= range.end);
    let at = styles.partition_point(|(run, _)| run.start < range.start);
    styles.insert(at, (range, style));
    styles
}

/// The text range of byte `slot` (0–15) in a row's hex text: two digits, a wider gap after 8.
fn hex_range(slot: usize) -> Range<usize> {
    let at = slot * 3 + usize::from(slot >= 8);
    at..at + 2
}

#[doc(hidden)]
pub fn hex_row(look: &RowLook<'_>, index: usize, line: &HexLine, cursor: &HexCursor) -> AnyElement {
    let (hex_paint, ascii_paint) = hex_ranges(&hex_slots(line.start, &look.painted));
    let is_bookmarked = look.is_bookmarked(line.start..line.start + HEX_WIDTH);
    let (mut hex, mut ascii) = (look.styled(hex_paint), look.styled(ascii_paint));
    let slot = cursor
        .at
        .and_then(|at| at.checked_sub(line.start))
        .filter(|slot| *slot < HEX_WIDTH)
        .and_then(|slot| usize::try_from(slot).ok());
    if let Some(slot) = slot.filter(|slot| *slot < line.row.ascii.len()) {
        hex = with_cursor(hex, hex_range(slot), cursor_style(look.t));
        ascii = with_cursor(ascii, slot..slot + 1, cursor_style(look.t));
    }
    hex_body(
        look,
        (index, line, is_bookmarked),
        (hex, ascii),
        cursor.pick.as_ref(),
    )
}

/// A row's hex or ASCII text; with `pick`, a click on byte `n` of the row picks that byte.
fn byte_text(
    id: (&'static str, usize),
    styled: StyledText,
    (line, ranges): (&HexLine, Vec<Range<usize>>),
    pick: Option<&PickByte>,
) -> AnyElement {
    let Some(pick) = pick.cloned() else {
        return styled.into_any_element();
    };
    let start = line.start;
    let text = InteractiveText::new(id, styled).on_click(ranges, move |slot, window, cx| {
        pick(start + slot as u64, window, cx);
    });
    div().id(id).test_support().child(text).into_any_element()
}

/// One hex row of the compare diff: the shared frame and aria label, tinted with the diff
/// tokens for the changed byte slots and a stronger tint for the current region.
#[doc(hidden)]
pub fn hex_row_tinted(
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
    hex_body(&look, (index, line, false), tint_styles(t, tints), None)
}

/// The one body both hex rows draw, at the look's height: gutter, offset, hex and ASCII.
fn hex_body(
    look: &RowLook<'_>,
    (index, line, is_bookmarked): (usize, &HexLine, bool),
    (hex_styles, ascii_styles): (Styles, Styles),
    pick: Option<&PickByte>,
) -> AnyElement {
    let row = &line.row;
    let label = format!("{}  {}  {}", row.offset, row.hex, row.ascii);
    let bytes = row.ascii.len();
    let hex = StyledText::new(row.hex.clone()).with_highlights(hex_styles);
    let ascii = StyledText::new(row.ascii.clone()).with_highlights(ascii_styles);
    let hex_ranges = (0..bytes).map(hex_range).collect();
    let ascii_ranges = (0..bytes).map(|slot| slot..slot + 1).collect();
    row_frame(look, index, label)
        .child(look.gutter(index, is_bookmarked))
        .child(
            div()
                .text_color(look.t.colors.muted)
                .child(row.offset.clone()),
        )
        .child(byte_text(
            ("paged-hex", index),
            hex,
            (line, hex_ranges),
            pick,
        ))
        .child(div().text_color(look.t.colors.muted).child(byte_text(
            ("paged-ascii", index),
            ascii,
            (line, ascii_ranges),
            pick,
        )))
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
        hex.push((hex_range(slot), style));
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

/// Builds a row's shown text and its paint runs, under syntax colours unless D10 blocks them.
fn row_highlights(
    look: &RowLook<'_>,
    text: &TextLook,
    line: &TextLine,
    is_hit: bool,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let cut = TextCut {
        start: line.text_from,
        end: line.end,
        shift: text.shift,
        limit: SHOWN_CHARS,
        layout: text.layout,
    };
    let (shown, runs) = shown_text(&line.text, &cut, &look.painted);
    let paints = look.styled(runs);
    let Some(theme) = text
        .syntax
        .as_ref()
        .filter(|_| !is_hit && !look.t.is_contrast)
    else {
        return (shown, paints);
    };
    let syntax: Vec<_> = syntax_runs(&line.text, text.shift, SHOWN_CHARS, &line.spans)
        .into_iter()
        .filter_map(|(range, name)| Some((range, theme.style.syntax.style(name)?)))
        .collect();
    let runs = overlay(&syntax, &paints);
    (shown, runs)
}

#[doc(hidden)]
pub fn text_row(look: &RowLook<'_>, text: &TextLook, index: usize, line: &TextLine) -> AnyElement {
    let number = line
        .line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let span = line.start..line.next.max(line.start + 1);
    let is_hit = look
        .painted
        .hit
        .is_some_and(|hit| span.contains(&hit.start));
    let (shown, runs) = row_highlights(look, text, line, is_hit);
    #[cfg(feature = "test-support")]
    super::run_probe::note(line.start, &shown, &runs);
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

#[doc(hidden)]
pub fn missing_row(look: &RowLook<'_>, index: usize) -> AnyElement {
    row_frame(look, index, String::new())
        .text_color(look.t.colors.muted)
        .into_any_element()
}

#[cfg(test)]
#[path = "row_view_tests.rs"]
mod tests;

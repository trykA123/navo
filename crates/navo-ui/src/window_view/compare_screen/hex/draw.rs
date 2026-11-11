//! Drawing of hex rows for the byte diff (V39): two columns, tinted changed bytes from the
//! diff tokens, ASCII, and a hatched gap cell when a side has none.

use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{ElementId, Role};
use navo_core::paging::{HEX_WIDTH, HexRow};
use navo_l10n::tr;

use super::fetch::ByteCache;
use super::hex_rows::{RowAt, RowSpan, Rows};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::{HexLine, hex_row_tinted};

/// The tint data one viewport draws with.
pub(super) struct Tints<'a> {
    pub(super) left: &'a [(Range<u64>, usize)],
    pub(super) right: &'a [(Range<u64>, usize)],
    pub(super) current: Option<usize>,
}

/// What a viewport needs to draw its rows.
pub(super) struct View<'a> {
    pub(super) t: &'a Theme,
    pub(super) range: Range<usize>,
    pub(super) rows: &'a Rows,
    pub(super) loaded: &'a ByteCache,
    pub(super) tints: Tints<'a>,
}

/// The visible rows, built from bytes already fetched; a missing row is a blank cell.
pub(super) fn rows(view: View<'_>) -> Vec<AnyElement> {
    let View {
        t,
        range,
        rows,
        loaded,
        tints,
    } = view;
    range
        .filter_map(|at| {
            let map = rows.row(at)?;
            Some(row_element((t, at), map, loaded, &tints))
        })
        .collect()
}

/// One side's changed ranges, sorted, with the region each belongs to.
pub(super) fn marks(
    regions: &[crate::compare_diff::ByteRegionView],
    right: bool,
) -> Vec<(Range<u64>, usize)> {
    let mut marks: Vec<(Range<u64>, usize)> = regions
        .iter()
        .enumerate()
        .filter_map(|(index, region)| {
            let range = match right {
                true => region.right.clone(),
                false => region.left.clone(),
            };
            (!range.is_empty()).then_some((range, index))
        })
        .collect();
    marks.sort_by_key(|(range, _)| range.start);
    marks
}

/// The byte slots of one row that a changed region covers, with whether it is the current one.
pub(super) fn tinted_slots(
    marks: &[(Range<u64>, usize)],
    span: RowSpan,
    current: Option<usize>,
) -> Vec<(usize, bool)> {
    let end = span
        .at
        .saturating_add(u64::try_from(span.len()).unwrap_or(HEX_WIDTH));
    let mut at = marks.partition_point(|(range, _)| range.end <= span.at);
    let mut slots = Vec::new();
    while let Some((range, index)) = marks.get(at) {
        if range.start >= end {
            break;
        }
        let from = range.start.max(span.at);
        let to = range.end.min(end);
        for byte in from..to {
            if let Ok(slot) = usize::try_from(byte.saturating_sub(span.at)) {
                slots.push((slot, current == Some(*index)));
            }
        }
        at += 1;
    }
    slots
}

/// The message of a pair whose bytes cannot be read.
pub(super) fn message(t: &Theme, text: SharedString) -> AnyElement {
    div()
        .id("compare-hex-message")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .p(t.spacing.xxl)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

/// One aligned row: the left and right hex rows, each with a gap cell when its side is absent.
fn row_element(
    (t, at): (&Theme, usize),
    map: RowAt,
    bytes: &ByteCache,
    tints: &Tints<'_>,
) -> AnyElement {
    let right = div()
        .flex_1()
        .min_w_0()
        .border_l_1()
        .border_color(t.colors.line)
        .pl(t.spacing.xl)
        .child(side_cell(
            (t, at, true),
            map.right,
            bytes.right(map.right),
            tints,
        ));
    div()
        .id(ElementId::from(("compare-hex-row", at)))
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(t.spacing.row_height)
        .child(div().flex_1().min_w_0().child(side_cell(
            (t, at, false),
            map.left,
            bytes.left(map.left),
            tints,
        )))
        .child(right)
        .into_any_element()
}

/// One side of a row: its offset, hex and ASCII, or a hatched gap when it has no bytes.
fn side_cell(
    (t, at, right): (&Theme, usize, bool),
    span: Option<RowSpan>,
    row: Option<HexRow>,
    tints: &Tints<'_>,
) -> AnyElement {
    let Some(span) = span else {
        return gap_cell(t, at, right);
    };
    let Some(row) = row else {
        return blank_cell(at, right);
    };
    let (marks, current) = match right {
        true => (tints.right, tints.current),
        false => (tints.left, tints.current),
    };
    let slots: Vec<(usize, bool)> = tinted_slots(marks, span, current)
        .into_iter()
        .filter(|(slot, _)| *slot < row.ascii.len())
        .collect();
    let index = at * 2 + usize::from(right);
    div()
        .id(ElementId::from(("compare-hex-side", index)))
        .flex()
        .items_center()
        .child(hex_row_tinted(
            t,
            index,
            &HexLine {
                start: span.at,
                row,
            },
            &slots,
        ))
        .into_any_element()
}

/// A blank cell for a row whose bytes are still being fetched; never a false gap.
fn blank_cell(at: usize, right: bool) -> AnyElement {
    div()
        .id(ElementId::from((
            "compare-hex-blank",
            at * 2 + usize::from(right),
        )))
        .test_support()
        .h_full()
        .w_full()
        .into_any_element()
}

/// A hatched cell for a side that has no bytes in this row.
fn gap_cell(t: &Theme, at: usize, right: bool) -> AnyElement {
    div()
        .id(ElementId::from((
            "compare-hex-gap",
            at * 2 + usize::from(right),
        )))
        .aria_label(tr("compare-gap"))
        .test_support()
        .h_full()
        .w_full()
        .bg(t.diff.gap_hatch)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::hex_rows::RowSpan;
    use super::tinted_slots;

    #[test]
    fn a_short_row_tints_only_the_slots_inside_the_region() {
        let marks = vec![(2u64..5u64, 0usize)];
        let slots = tinted_slots(&marks, RowSpan { at: 0, len: 8 }, None);
        assert_eq!(slots, vec![(2, false), (3, false), (4, false)]);
    }

    #[test]
    fn the_current_region_gets_the_stronger_tint() {
        let marks = vec![(0u64..16u64, 3usize)];
        let slots = tinted_slots(&marks, RowSpan { at: 0, len: 16 }, Some(3));
        assert!(slots.iter().all(|(_, current)| *current));
        assert_eq!(slots.len(), 16);
    }

    #[test]
    fn a_row_never_tints_bytes_past_its_own_length() {
        let marks = vec![(0u64..16u64, 0usize)];
        let slots = tinted_slots(&marks, RowSpan { at: 0, len: 8 }, None);
        assert_eq!(slots.len(), 8);
    }
}

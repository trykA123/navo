//! The binary card's hex body and status words: the window around its first changed region,
//! laid out like the file screen's hex view at the card's fixed height (D26.5, D26.8).

use std::ops::Range;

use navo_core::paging::{HEX_WIDTH, hex_row};
use navo_l10n::{t, tr};

use crate::compare_diff::{
    ByteProgress, BytesCard, CardDetail, CardScan, HEX_CARD_ROWS, HexWindow,
};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::{HexLine, hex_row_tinted};

/// What one side of a card row draws: its first offset, its bytes, the left or right range of
/// the changed region and whether its offsets need 16 hex digits.
struct Side<'a> {
    start: u64,
    bytes: &'a [u8],
    changed: &'a Range<u64>,
    wide: bool,
}

/// The binary detail's body rows: the card's hex window, or nothing while its bytes are still
/// being read (D26.8). Any other detail draws no hex rows.
pub(super) fn body(t: &Theme, detail: Option<&CardDetail>) -> Vec<AnyElement> {
    let Some(CardDetail::Bytes(card)) = detail else {
        return Vec::new();
    };
    let Some(window) = card.window.as_ref() else {
        return Vec::new();
    };
    vec![hex_body(t, window).into_any_element()]
}

/// The card's body: `HEX_CARD_ROWS` rows of `t.spacing.row_height`, whatever the window holds.
/// A row past a side's bytes is an empty cell of the same height, so the card keeps its fixed
/// height and a new window splices no row (D26.5).
pub(super) fn hex_body(t: &Theme, window: &HexWindow) -> Div {
    let mut body = div().flex().flex_col().w_full().items_start();
    for at in 0..HEX_CARD_ROWS {
        body = body.child(card_row(t, at, window));
    }
    body
}

/// One card row: the left side, then the right one behind a hairline, as the file screen's hex
/// view lays out its columns. Cell indexes are `row × 2 + side`, the shared hex-row numbering.
fn card_row(t: &Theme, at: usize, window: &HexWindow) -> AnyElement {
    let left = at * 2;
    div()
        .flex()
        .w_full()
        .flex_none()
        .h(t.spacing.row_height)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(cell(t, &side(window, false, at), left)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .border_l_1()
                .border_color(t.colors.line)
                .child(cell(t, &side(window, true, at), left + 1)),
        )
        .into_any_element()
}

/// One side of a card row: its tinted hex row, or an empty cell when the window holds no bytes
/// that far. Slots past the side's own bytes are never tinted.
fn cell(t: &Theme, side: &Side<'_>, index: usize) -> AnyElement {
    if side.bytes.is_empty() {
        return div().w_full().h(t.spacing.row_height).into_any_element();
    }
    let tints: Vec<(usize, bool)> = window_tints(side.start, side.changed)
        .into_iter()
        .filter(|(slot, _)| *slot < side.bytes.len())
        .collect();
    let line = HexLine {
        start: side.start,
        row: hex_row(side.start, side.bytes, side.wide),
    };
    hex_row_tinted(t, index, &line, &tints)
}

/// The window's `right` side at row `at`, with the bytes that row shows and none past the window.
fn side(window: &HexWindow, right: bool, at: usize) -> Side<'_> {
    let (base, bytes, changed, wide) = match right {
        true => (
            window.right_start,
            window.right.as_slice(),
            &window.changed_right,
            window.wide.1,
        ),
        false => (
            window.left_start,
            window.left.as_slice(),
            &window.changed_left,
            window.wide.0,
        ),
    };
    let offset = u64::try_from(at)
        .unwrap_or(u64::MAX)
        .saturating_mul(HEX_WIDTH);
    let from = usize::try_from(offset).unwrap_or(usize::MAX);
    let width = usize::try_from(HEX_WIDTH).unwrap_or(usize::MAX);
    Side {
        start: base.saturating_add(offset),
        bytes: bytes.get(from..from.saturating_add(width)).unwrap_or(&[]),
        changed,
        wide,
    }
}

/// The slots of the 16-byte row starting at `row_start` that `changed` covers: slot = offset −
/// `row_start`, cut at the row's end, every one marked as changed (D26.8). A range that misses
/// the row gives none; the caller drops slots past the bytes it actually shows.
pub(super) fn window_tints(row_start: u64, changed: &Range<u64>) -> Vec<(usize, bool)> {
    let from = changed.start.max(row_start);
    let to = changed.end.min(row_start.saturating_add(HEX_WIDTH));
    (from..to)
        .filter_map(|byte| usize::try_from(byte - row_start).ok())
        .map(|slot| (slot, true))
        .collect()
}

/// What a binary card says on its status line: where its first change is, that the files are
/// equal, or how far the byte scan got (D26.4, D26.8). A card stopped as full without a window
/// has nothing to point at, so it keeps showing its progress.
pub(super) fn hex_status(card: &BytesCard) -> String {
    match card.scan {
        CardScan::Full => card.window.as_ref().map_or_else(
            || byte_status(card.progress),
            |window| {
                t!(
                    "compare-card-hex-first",
                    offset = format!("0x{:X}", window.changed_left.start)
                )
            },
        ),
        CardScan::Done => tr("compare-screen-no-changes"),
        CardScan::Running => byte_status(card.progress),
    }
}

/// How far the byte scan got: a percent once its total is known, plain waiting text before (D26.4).
fn byte_status(progress: Option<ByteProgress>) -> String {
    let percent =
        |progress: ByteProgress| (progress.done.saturating_mul(100) / progress.total).min(100);
    match progress {
        Some(progress) if progress.total > 0 => {
            t!(
                "compare-screen-comparing-percent",
                percent = percent(progress)
            )
        }
        _ => tr("compare-screen-comparing"),
    }
}

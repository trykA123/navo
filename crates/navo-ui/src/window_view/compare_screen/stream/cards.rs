//! The heavy card's fixed-height detail body: its status line, then the large card's hunk rows
//! (D26.5, D26.7) or the binary card's hex window (D26.8). A detail never changes the height, so
//! a new one splices no rows (D26.3).

use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use navo_l10n::{t, tr};

use super::Thumbs;
use crate::compare_diff::card::{LINE_HEIGHT, NOTE_HEIGHT, frame};
use crate::compare_diff::{
    CARD_HUNKS, CardDetail, CardScan, HEX_CARD_ROWS, LargeCard, LargeProgress, PairKind,
    StreamModel,
};
use crate::prelude::*;
use crate::tokens::Theme;

mod hex;
mod image;

/// What a detail row needs: the theme, the model holding the card and its detail, and the GPU
/// textures uploaded for the cards near the view (D26.3).
#[derive(Clone, Copy)]
pub(super) struct DetailCtx<'a> {
    pub t: &'a Theme,
    pub model: &'a StreamModel,
    pub thumbs: &'a Thumbs,
}

/// The height a heavy card's body always draws at, detail or none (D26.5): one status line plus
/// `CARD_HUNKS` text lines for a large or image pair, `HEX_CARD_ROWS` hex rows for a binary one.
/// A light kind never asks for a detail; it falls back to the large card's height.
#[must_use]
pub(super) fn detail_height(t: &Theme, kind: PairKind) -> Pixels {
    let body = match kind {
        PairKind::Binary => t.spacing.row_height * HEX_CARD_ROWS as f32,
        _ => px(CARD_HUNKS as f32 * LINE_HEIGHT),
    };
    px(NOTE_HEIGHT) + body
}

/// The detail row of item `item`: a frame of the card's own height, its status line and the body
/// its kind has — line ranges for a large pair (D26.7), hex rows for a binary one (D26.8) or the
/// two thumbnails of an image pair (D26.5). Fails to an empty frame when the card was evicted
/// between building the rows and drawing them.
pub(super) fn detail_row(ctx: &DetailCtx<'_>, item: usize) -> AnyElement {
    let t = ctx.t;
    let id = ctx.model.items()[item].id;
    let Some(card) = ctx.model.card(id) else {
        return frame(t, true).into_any_element();
    };
    let held = ctx.model.detail(id).map(|detail| &**detail);
    let (text, side) = status_of(held);
    let thumbs = matches!(held, Some(CardDetail::Image(_)))
        .then(|| image::image_body(ctx, id).into_any_element());
    frame(t, true)
        .flex_col()
        .items_start()
        .h(detail_height(t, card.kind))
        .id(("compare-card-detail", id))
        .test_support()
        .child(status_row(t, text, side))
        .children(hunk_rows(t, held))
        .children(hex::body(t, held))
        .children(thumbs)
        .into_any_element()
}

/// The status line of a card: `text` at the left and `side` at the right when the job has a
/// second thing to say. It is the note row of D26.5's fixed heights.
fn status_row(t: &Theme, text: String, side: Option<String>) -> Div {
    div()
        .flex()
        .flex_none()
        .w_full()
        .items_center()
        .gap(t.spacing.md)
        .h(px(NOTE_HEIGHT))
        .px(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(SharedString::from(text))
        .child(div().flex_1())
        .children(side.map(SharedString::from))
}

/// The two texts of a card's status line, from the detail held for its card: the large card's
/// words, the failure text, or "Comparing…" while no detail has arrived (D26.4).
fn status_of(detail: Option<&CardDetail>) -> (String, Option<String>) {
    match detail {
        None => (tr("compare-screen-comparing"), None),
        Some(CardDetail::Large(card)) => large_status(card),
        Some(CardDetail::Failed(why)) => (why.clone(), None),
        Some(CardDetail::Bytes(card)) => (hex::hex_status(card), None),
        Some(CardDetail::Image(card)) => (image::image_status(card), None),
    }
}

/// What a large card says once its detail arrived: how many changes it found and, while the scan
/// runs, how far it got (D26.4). The right-hand text is `None` when the card has its answer.
pub(super) fn large_status(card: &LargeCard) -> (String, Option<String>) {
    let hunks = u64::from(card.found);
    match card.scan {
        CardScan::Running => {
            let side = card
                .progress
                .and_then(percent)
                .map_or(tr("compare-screen-comparing"), |percent| {
                    t!("compare-screen-comparing-percent", percent = percent)
                });
            (t!("compare-card-large-found", hunks = hunks), Some(side))
        }
        CardScan::Done if card.found == 0 => (tr("compare-screen-no-changes"), None),
        CardScan::Done => (t!("compare-screen-changes", total = hunks), None),
        CardScan::Full => (t!("compare-card-large-full", hunks = hunks), None),
    }
}

/// The share of a running scan already done, or `None` while its total is still unknown.
fn percent(progress: LargeProgress) -> Option<u64> {
    (progress.total > 0).then(|| (progress.done.saturating_mul(100) / progress.total).min(100))
}

/// The large card's hunk rows: at most `CARD_HUNKS` of them, one line range per row (D26.4).
/// Any other detail draws only its status line, so this is empty.
fn hunk_rows(t: &Theme, detail: Option<&CardDetail>) -> Vec<AnyElement> {
    let Some(CardDetail::Large(card)) = detail else {
        return Vec::new();
    };
    card.hunks
        .iter()
        .take(CARD_HUNKS)
        .enumerate()
        .map(|(at, hunk)| {
            let text = t!(
                "compare-card-large-hunk",
                left = rows_text(&hunk.left_rows),
                right = rows_text(&hunk.right_rows)
            );
            div()
                .id(("compare-card-large-hunk", at))
                .aria_label(text.clone())
                .test_support()
                .flex()
                .flex_none()
                .w_full()
                .items_center()
                .h(px(LINE_HEIGHT))
                .px(t.spacing.lg)
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.text)
                .child(text)
                .into_any_element()
        })
        .collect()
}

/// The line numbers a hunk covers, 1-based: "a" for one line, "a–b" for several (D26.7). A range
/// the scan has not sized yet shows the line it started at.
pub(super) fn rows_text(rows: &Range<u64>) -> String {
    let first = rows.start + 1;
    if rows.end > first {
        format!("{first}–{}", rows.end)
    } else {
        format!("{first}")
    }
}

#[cfg(test)]
mod tests;

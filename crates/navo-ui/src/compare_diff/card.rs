//! The Stream's card rows: header, lines, gap, more, placeholder, summary and rules group.
//! Presentational only: each row is built from its `StreamRow`, the model and the theme.

mod header;
mod line;

use navo_l10n::{t, tr};

use super::copy_bar::{BarRow, CardBars, hunk_row};
use super::hooks::{Card, CardRow, Layout, PairKind};
use super::rows::{Half, StreamRow};
use super::stream_model::StreamModel;
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

pub(crate) use header::badge;
use header::{accent_bar, header};

/// Height of a header row, in px.
const HEADER_HEIGHT: f32 = 36.;
/// Height of a gap, more, placeholder or summary row, in px.
const NOTE_HEIGHT: f32 = 26.;
/// Size of the icons in a header, in px.
const ICON: f32 = 14.;
/// Size of the status mark badge, in px.
const BADGE: f32 = 18.;

/// What every card row needs: the theme, the Stream model, the layout and the copy bars.
#[derive(Clone, Copy)]
pub struct CardCtx<'a> {
    pub t: &'a Theme,
    pub model: &'a StreamModel,
    pub layout: Layout,
    /// The hover bars of copyable changes; `None` draws none.
    pub bars: Option<&'a CardBars>,
}

/// Builds the element for `rows[at]`. The card's last row gets the rounded bottom corners.
pub fn stream_row_view(rows: &[StreamRow], at: usize, ctx: &CardCtx<'_>) -> AnyElement {
    let t = ctx.t;
    let last = rows
        .get(at + 1)
        .is_none_or(|next| matches!(next, StreamRow::Header(_) | StreamRow::RulesGroup));
    match rows[at] {
        StreamRow::Header(item) => header(ctx, item, last).into_any_element(),
        StreamRow::Line {
            item,
            card_row,
            half,
        } => card_line(ctx, (at, item, card_row), half, last),
        StreamRow::Gap { lines, .. } => note(t, last, t!("compare-card-gap", lines = lines)),
        StreamRow::More { hunks, .. } => note(t, last, t!("compare-card-more", hunks = hunks)),
        StreamRow::Others { hunks, .. } => note(t, last, t!("compare-card-others", hunks = hunks)),
        StreamRow::Placeholder(_) => note(t, last, tr("compare-screen-comparing")),
        StreamRow::Summary(item) => summary(t, ctx.model, item, last),
        // Until its own body is drawn, a detail row falls back to the card's summary (D26.5).
        StreamRow::Detail(item) => summary(t, ctx.model, item, last),
        StreamRow::RulesGroup => rules_group(t, ctx.model.rules()).into_any_element(),
    }
}

/// A card line looked up from the model; an empty row if the card was evicted meanwhile.
fn card_line(
    ctx: &CardCtx<'_>,
    (at, item, card_row): (usize, usize, u32),
    half: Half,
    last: bool,
) -> AnyElement {
    let model = ctx.model;
    let id = model.items()[item].id;
    let card = model.card(id);
    let row = card.and_then(|card| card.rows.get(card_row as usize));
    let Some(CardRow::Line {
        row,
        hunk,
        left,
        right,
    }) = row
    else {
        return note(ctx.t, last, String::new());
    };
    let cursor = model.cursor();
    let look = line::Look {
        layout: ctx.layout,
        half,
        current: cursor.item == item && cursor.hunk == Some(*hunk),
        last,
    };
    let text = line::Texts {
        left: left.as_deref(),
        right: right.as_deref(),
    };
    let drawn = line::view(ctx.t, look, *row, text).into_any_element();
    match ctx.bars.zip(card).filter(|(_, card)| copyable(card, *hunk)) {
        Some((bars, card)) => {
            let first = half != Half::Right && starts_hunk(card, card_row);
            with_bar(ctx.t, drawn, bars, ((at, id, *hunk), first))
        }
        None => drawn,
    }
}

/// Whether change `hunk` of `card` can be copied: a text card whose change has a digest.
fn copyable(card: &Card, hunk: u32) -> bool {
    card.kind == PairKind::Text
        && card
            .hunks
            .get(hunk as usize)
            .is_some_and(|view| view.digest.is_some())
}

/// Whether card row `card_row` is the first line of its change.
fn starts_hunk(card: &Card, card_row: u32) -> bool {
    let Some(CardRow::Line { hunk, .. }) = card.rows.get(card_row as usize) else {
        return false;
    };
    card_row
        .checked_sub(1)
        .and_then(|before| card.rows.get(before as usize))
        .is_none_or(|row| !matches!(row, CardRow::Line { hunk: h, .. } if h == hunk))
}

/// Wraps a card line (row `at` of the list) of change `hunk` of item `id` so it reports hover
/// and, on the change's first line, carries the bar.
fn with_bar(
    t: &Theme,
    row: AnyElement,
    bars: &CardBars,
    ((at, id, hunk), first): ((usize, u64, u32), bool),
) -> AnyElement {
    let (on_hover, on_copy) = (bars.on_hover.clone(), bars.on_copy.clone());
    let bar = BarRow {
        id: ("compare-card-row", at),
        first,
        hovered: bars.hovered == Some((id, hunk)),
        animate: bars.animate,
        on_hover: std::rc::Rc::new(move |on, cx| on_hover(id, hunk, on, cx)),
        on_copy: std::rc::Rc::new(move |direction, window, cx| {
            on_copy(id, hunk, direction, window, cx);
        }),
    };
    hunk_row(t, row, bar)
}

/// The frame every card row shares: card surface, side edges, rounded bottom on the last row.
fn frame(t: &Theme, last: bool) -> Div {
    div()
        .relative()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .bg(t.surfaces.card)
        .border_l_1()
        .border_r_1()
        .border_color(t.surfaces.card_edge)
        .when(last, |d| d.border_b_1().rounded_b(t.radii.card))
}

/// A one-line note row (gap, more, placeholder), centred and muted.
fn note(t: &Theme, last: bool, text: String) -> AnyElement {
    frame(t, last)
        .h(px(NOTE_HEIGHT))
        .px(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(SharedString::from(text))
        .into_any_element()
}

/// What a card without rows is, in words: large (with sizes), binary, image, one-sided or why
/// it is unavailable; a text card or one not built yet has no changes to show.
pub(crate) fn kind_text(kind: Option<PairKind>) -> String {
    match kind {
        Some(PairKind::Large { left, right }) => t!(
            "compare-card-summary-large",
            left = format::size(left),
            right = format::size(right)
        ),
        Some(PairKind::Binary) => tr("compare-card-summary-binary"),
        Some(PairKind::Image) => tr("compare-card-summary-image"),
        Some(PairKind::OneSided { left: true }) => tr("compare-card-summary-left"),
        Some(PairKind::OneSided { left: false }) => tr("compare-card-summary-right"),
        Some(PairKind::Unavailable(reason)) => tr(reason),
        Some(PairKind::Text) | None => tr("compare-card-summary-text"),
    }
}

/// The summary of a card without rows: its kind, sizes where known, and "Open with F3".
fn summary(t: &Theme, model: &StreamModel, at: usize, last: bool) -> AnyElement {
    let text = kind_text(model.card(model.items()[at].id).map(|card| card.kind));
    frame(t, last)
        .h(px(NOTE_HEIGHT + 6.))
        .gap(t.spacing.md)
        .px(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(SharedString::from(text))
        .child(div().flex_1())
        .child(
            div()
                .flex_none()
                .text_color(t.colors.accent_text)
                .child(tr("compare-card-open-f3")),
        )
        .into_any_element()
}

/// The closing group: how many files are the same after the rules.
fn rules_group(t: &Theme, count: usize) -> Div {
    frame(t, true)
        .border_t_1()
        .rounded(t.radii.card)
        .h(px(NOTE_HEIGHT + 6.))
        .px(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(t!("compare-card-rules", count = count))
}

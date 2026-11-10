//! A Stream card's header row: fold chevron, status mark, file icon, path, `+n −m` and the
//! Viewed tick, with the accent bar that marks the current stop.

use navo_fs::EntryKind;
use navo_l10n::tr;

use super::{BADGE, CardCtx, HEADER_HEIGHT, ICON, frame};
use crate::browser::look_of;
use crate::compare_diff::hooks::Card;
use crate::compare_diff::stream_model::StreamItem;
use crate::compare_list::tone;
use crate::file_list::kind;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

/// The status mark badge: a glyph on the tone's wash.
pub(crate) fn badge(t: &Theme, item: &StreamItem) -> Div {
    let (tone_of, mark) = look_of(item.status);
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(px(BADGE))
        .h(px(BADGE))
        .rounded(t.radii.xs)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .font_weight(FontWeight::BOLD)
        .text_color(tone::text(t, tone_of))
        .when_some(tone::wash(t, tone_of), Styled::bg)
        .child(mark)
}

/// The path with its folder dimmed and the file name bold.
fn path_view(t: &Theme, path: &str) -> Div {
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    div()
        .flex()
        .min_w_0()
        .overflow_hidden()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .when(!dir.is_empty(), |d| {
            d.child(
                div()
                    .text_color(t.colors.muted)
                    .child(SharedString::from(format!("{dir}/"))),
            )
        })
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text)
                .child(SharedString::from(name.to_owned())),
        )
}

/// `+n −m`, added in the add colour and removed in the delete colour.
fn counts(t: &Theme, card: &Card) -> Div {
    div()
        .flex_none()
        .flex()
        .gap(t.spacing.sm)
        .font_family(t.typography.numbers_font.clone())
        .text_size(t.typography.small)
        .child(
            div()
                .text_color(t.diff.add.text)
                .child(format!("+{}", card.added)),
        )
        .child(
            div()
                .text_color(t.diff.delete.text)
                .child(format!("\u{2212}{}", card.removed)),
        )
}

/// The Viewed tick: a small box, checked when the file is viewed, and its label.
fn viewed_tick(t: &Theme, viewed: bool) -> Div {
    let boxed = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(px(ICON))
        .h(px(ICON))
        .rounded(t.radii.xs)
        .border_1()
        .border_color(if viewed {
            t.colors.accent
        } else {
            t.colors.line
        })
        .when(viewed, |d| {
            d.bg(t.colors.accent)
                .child(icon(IconName::Check, px(ICON - 4.), t.colors.on_button))
        });
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(boxed)
        .child(tr("compare-card-viewed"))
}

/// A card header: fold chevron, status mark, file icon, path, counts and the Viewed tick.
pub(super) fn header(ctx: &CardCtx<'_>, at: usize, last: bool) -> Div {
    let (t, model) = (ctx.t, ctx.model);
    let cursor = model.cursor();
    let item = &model.items()[at];
    let open = model.is_open(item.id);
    let viewed = model.is_viewed(item.id);
    let (_, ext) = kind::split_name(&item.path, EntryKind::File);
    let (glyph, glyph_color) = kind::icon(EntryKind::File, ext, &t.colors);
    let chevron = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let current = cursor.item == at && cursor.hunk.is_none();
    frame(t, last)
        .h(px(HEADER_HEIGHT))
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .border_t_1()
        .rounded_t(t.radii.card)
        .when(open && !last, |d| {
            d.border_b_1().border_color(t.colors.line)
        })
        .when(viewed, |d| d.opacity(0.72))
        .when(current, |d| d.child(accent_bar(t)))
        .child(icon(chevron, px(ICON), t.colors.muted))
        .child(badge(t, item))
        .child(icon(glyph, px(ICON), glyph_color))
        .child(path_view(t, &item.path))
        .when_some(model.card(item.id), |d, card| d.child(counts(t, card)))
        .child(div().flex_1())
        .child(viewed_tick(t, viewed))
}

/// The accent bar on the left edge that marks the current stop.
pub(super) fn accent_bar(t: &Theme) -> Div {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(2.))
        .bg(t.colors.accent)
}

use rusty_ui::prelude::*;
use rusty_ui::primitives::{IconName, icon};
use rusty_ui::tokens::Theme;

use crate::sections::section;

const CARD_HEIGHT: f32 = 150.;
const KEY_HEIGHT: f32 = 30.;
const CAP_HEIGHT: f32 = 20.;
const PILL_ICON: f32 = 24.;
const PILL_TRACK: f32 = 72.;
const JOB_CARD_WIDTH: f32 = 280.;
const TILE_WIDTH: f32 = 80.;
const TILE_HEIGHT: f32 = 70.;
const SHEET_SAMPLE_WIDTH: f32 = 232.;

fn text(color: Hsla, label: &'static str) -> Div {
    div().text_color(color).child(label)
}

fn progress(t: &Theme, width: Pixels, done: f32) -> Div {
    let s = &t.surfaces;
    div()
        .flex_none()
        .w(width)
        .h(t.spacing.xs)
        .rounded(t.radii.pill)
        .bg(s.progress_track)
        .child(
            div()
                .h_full()
                .w(relative(done))
                .rounded(t.radii.pill)
                .bg(s.progress_fill),
        )
}

fn field(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(t.spacing.row_height)
        .px(t.spacing.md)
        .rounded(t.radii.md)
        .bg(t.surfaces.field)
        .child(icon(IconName::Search, t.spacing.xl, t.colors.muted))
        .child(text(t.colors.muted, "Filter"))
}

fn card(t: &Theme, is_active: bool, path: &'static str) -> Div {
    let rows = ["Cargo.toml", "README.md", "src"];
    let shadow = if is_active {
        &t.shadows.card_active
    } else {
        &t.shadows.card
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(t.spacing.sm)
        .h(px(CARD_HEIGHT))
        .p(t.spacing.xl)
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .shadow(shadow.clone())
        .child(text(t.colors.muted, path))
        .child(field(t))
        .children(rows.map(|row| text(t.colors.text, row)))
}

fn cap(t: &Theme, key: &'static str, is_lit: bool) -> Div {
    let s = &t.surfaces;
    let base = div()
        .flex()
        .items_center()
        .h(px(CAP_HEIGHT))
        .px(t.spacing.xs)
        .rounded(t.radii.sm)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.key);
    if is_lit {
        base.bg(s.keycap_lit).text_color(s.keycap_lit_label)
    } else {
        base.bg(s.keycap_face)
            .text_color(s.keycap_label)
            .border_1()
            .border_color(t.colors.line)
            .shadow(t.shadows.keycap.clone())
    }
    .child(key)
}

fn key(t: &Theme, key: &'static str, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .h(px(KEY_HEIGHT))
        .px(t.spacing.xs)
        .rounded(t.radii.keycap)
        .child(cap(t, key, false))
        .child(text(t.colors.text, label))
}

fn pill(t: &Theme) -> Div {
    let s = &t.surfaces;
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(KEY_HEIGHT))
        .pl(t.spacing.xxs)
        .pr(t.spacing.lg)
        .rounded(t.radii.pill)
        .bg(s.pill)
        .shadow(t.shadows.pill.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(PILL_ICON))
                .rounded(t.radii.pill)
                .bg(s.current)
                .child(icon(IconName::Copy, t.spacing.xl, s.pill_label)),
        )
        .child(text(t.colors.text, "photos"))
        .child(progress(t, px(PILL_TRACK), 0.42))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(s.pill_label)
                .child("42 %"),
        )
}

fn key_row(t: &Theme) -> Div {
    let keys = [
        ("F3", "View"),
        ("F4", "Edit"),
        ("F5", "Copy"),
        ("F6", "Move"),
        ("F10", "Quit"),
    ];
    let layer = div()
        .flex()
        .items_center()
        .gap(t.spacing.xxs)
        .px(t.spacing.xxs)
        .rounded(t.radii.md)
        .bg(t.surfaces.layer_tint)
        .children(keys.map(|(cap, label)| key(t, cap, label)));
    let body = div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(cap(t, "Shift", true))
        .child(cap(t, "Ctrl", false))
        .child(layer)
        .child(pill(t));
    section(t, "KEYCAPS, A LIT LAYER, AND THE COPY PILL", body)
}

fn job_card(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .w(px(JOB_CARD_WIDTH))
        .py(t.spacing.xl)
        .px(t.spacing.xxl)
        .rounded(t.radii.job_card)
        .bg(t.surfaces.job_card)
        .shadow(t.shadows.job_card.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Copying 128 files")
                .child(icon(IconName::ChevronDown, t.spacing.xxl, t.colors.muted)),
        )
        .child(text(t.colors.muted, "IMG_0412.jpg · 34 of 128"))
        .child(progress(t, px(JOB_CARD_WIDTH) - t.spacing.xxl * 2., 0.35))
        .child(text(t.colors.muted, "1.2 of 3.4 GB · 2 min left"))
}

fn tile(t: &Theme, glyph: IconName, label: &'static str, is_on: bool) -> Div {
    let s = &t.surfaces;
    let (face, edge, ink) = if is_on {
        (s.layer_tint, t.colors.accent, t.colors.accent_text)
    } else {
        (s.tile, s.tile_edge, t.colors.muted)
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .w(px(TILE_WIDTH))
        .h(px(TILE_HEIGHT))
        .py(t.spacing.md)
        .px(t.spacing.lg)
        .rounded(t.radii.card)
        .border_1()
        .border_color(edge)
        .bg(face)
        .child(icon(glyph, t.spacing.xxl, ink))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if is_on { ink } else { t.colors.text })
                .child(label),
        )
}

fn menu(t: &Theme) -> Div {
    div()
        .flex()
        .gap(t.spacing.md)
        .p(t.spacing.md)
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.sheet)
        .shadow(t.shadows.menu.clone())
        .child(tile(t, IconName::Moon, "Dark", true))
        .child(tile(t, IconName::Layers, "Panes", false))
        .child(tile(t, IconName::PanelRight, "Preview", false))
}

fn sheet(t: &Theme) -> Div {
    let s = &t.surfaces;
    let panel = div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(relative(0.64))
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .p(t.spacing.xl)
        .border_l_1()
        .border_color(t.colors.line)
        .bg(s.sheet)
        .shadow(t.shadows.sheet.clone())
        .child(div().font_weight(FontWeight::SEMIBOLD).child("Settings"))
        .child(text(t.colors.muted, "Theme · Dark"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.xs)
                .px(t.spacing.md)
                .rounded(t.radii.pill)
                .border_1()
                .border_color(t.colors.line)
                .bg(s.sheet)
                .shadow(t.shadows.chip.clone())
                .text_size(t.typography.tiny)
                .child(
                    div()
                        .size(t.spacing.sm)
                        .rounded(t.radii.pill)
                        .bg(t.colors.ok),
                )
                .child("Live"),
        );
    div()
        .relative()
        .w(px(SHEET_SAMPLE_WIDTH))
        .h(px(CARD_HEIGHT * 0.8))
        .overflow_hidden()
        .rounded(t.radii.card)
        .bg(s.card)
        .p(t.spacing.xl)
        .child(text(t.colors.text, "Passive pane"))
        .child(div().absolute().inset_0().bg(s.scrim))
        .child(panel)
}

fn overlays(t: &Theme) -> Div {
    div()
        .flex()
        .items_start()
        .gap(t.spacing.xxl)
        .child(section(t, "JOB CARD", job_card(t)))
        .child(section(t, "MENU TILES", menu(t)))
        .child(section(t, "SHEET ON SCRIM", sheet(t)))
}

pub fn render(t: &Theme) -> Div {
    let cards = div()
        .flex()
        .gap(t.spacing.xxl)
        .child(card(t, true, "C: › Users › admin › commander"))
        .child(card(t, false, "D: › Backup"));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.xxl * 1.5)
        .p(t.spacing.xxl * 1.5)
        .bg(t.surfaces.mica())
        .child(section(t, "CARDS ON MICA, ACTIVE AND PASSIVE", cards))
        .child(key_row(t))
        .child(overlays(t))
}

use rusty_ui::prelude::*;
use rusty_ui::primitives::{IconName, icon};
use rusty_ui::tokens::Theme;

use crate::sections::section;

const CARD_HEIGHT: f32 = 150.;
const KEYCAP_HEIGHT: f32 = 30.;
const PILL_TRACK: f32 = 72.;
const JOB_CARD_WIDTH: f32 = 300.;
const TILE_WIDTH: f32 = 84.;
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
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(t.spacing.sm)
        .h(px(CARD_HEIGHT))
        .p(t.spacing.xl)
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .shadow(t.shadows.card.clone())
        .border_2()
        .border_color(if is_active {
            t.surfaces.card_ring
        } else {
            t.surfaces.card
        })
        .child(text(t.colors.muted, path))
        .child(field(t))
        .children(rows.map(|row| text(t.colors.text, row)))
}

fn keycap(t: &Theme, key: &'static str, label: &'static str, is_lit: bool) -> Div {
    let s = &t.surfaces;
    let (face, edge, ink, key_ink) = if is_lit {
        (
            s.keycap_lit,
            s.keycap_lit,
            s.keycap_lit_label,
            s.keycap_lit_label,
        )
    } else {
        (s.keycap_face, s.keycap_edge, s.keycap_label, t.colors.muted)
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(KEYCAP_HEIGHT))
        .px(t.spacing.lg)
        .rounded(t.radii.keycap)
        .bg(face)
        .border_1()
        .border_b_2()
        .border_color(edge)
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.key)
                .text_color(key_ink)
                .child(key),
        )
        .child(text(ink, label))
}

fn pill(t: &Theme) -> Div {
    let s = &t.surfaces;
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(KEYCAP_HEIGHT))
        .px(t.spacing.xl)
        .rounded(t.radii.pill)
        .bg(s.pill)
        .border_1()
        .border_color(s.pill_ring)
        .child(icon(IconName::Copy, t.spacing.xl, t.colors.accent_text))
        .child(text(t.colors.text, "Copying photos"))
        .child(progress(t, px(PILL_TRACK), 0.42))
        .child(text(t.colors.muted, "42 % · 1 min"))
}

fn key_row(t: &Theme) -> Div {
    let keys = [
        ("", "Shift", true),
        ("F3", "View", false),
        ("F4", "Edit", false),
        ("F5", "Copy", false),
        ("F6", "Move", false),
        ("F10", "Quit", false),
    ];
    let body = div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .children(keys.map(|(key, label, is_lit)| keycap(t, key, label, is_lit)))
        .child(pill(t));
    section(t, "KEYCAPS, LIT AND UNLIT, AND THE COPY PILL", body)
}

fn job_card(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .w(px(JOB_CARD_WIDTH))
        .p(t.spacing.xxl)
        .rounded(t.radii.card)
        .bg(t.surfaces.job_card)
        .shadow(t.shadows.job_card.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Copying 128 files to D:\\Backup")
                .child(icon(IconName::ChevronDown, t.spacing.xxl, t.colors.muted)),
        )
        .child(text(
            t.colors.muted,
            "34 of 128 · 1.2 of 3.4 GB · 2 min left",
        ))
        .child(progress(t, px(JOB_CARD_WIDTH) - t.spacing.xxl * 2., 0.35))
}

fn tile(t: &Theme, glyph: IconName, label: &'static str, is_on: bool) -> Div {
    let (face, ink) = if is_on {
        (t.colors.selection, t.colors.accent_text)
    } else {
        (t.surfaces.tile, t.colors.muted)
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .w(px(TILE_WIDTH))
        .p(t.spacing.lg)
        .rounded(t.radii.card)
        .bg(face)
        .child(icon(glyph, t.spacing.xxl, ink))
        .child(text(t.colors.text, label))
}

fn sheet(t: &Theme) -> Div {
    let s = &t.surfaces;
    let panel = div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(relative(0.62))
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .p(t.spacing.xl)
        .rounded_l(t.radii.sheet)
        .bg(s.sheet)
        .shadow(t.shadows.job_card.clone())
        .child(div().font_weight(FontWeight::SEMIBOLD).child("Settings"))
        .child(text(t.colors.muted, "Theme · Dark"))
        .child(text(t.colors.muted, "Density · Compact"));
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
    let tiles = div()
        .flex()
        .gap(t.spacing.sm)
        .child(tile(t, IconName::Moon, "Dark", true))
        .child(tile(t, IconName::Layers, "Panes", false))
        .child(tile(t, IconName::PanelRight, "Preview", false));
    div()
        .flex()
        .items_start()
        .gap(t.spacing.xxl * 1.5)
        .child(section(t, "JOB CARD", job_card(t)))
        .child(section(t, "TILES", tiles))
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

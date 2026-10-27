use rusty_ui::prelude::*;
use rusty_ui::tokens::Theme;

use crate::gallery::Page;
use crate::palettes_page;
use crate::sections::section;
use crate::tokens_sample;

const SWATCH_WIDTH: f32 = 168.;

fn swatch(t: &Theme, name: &'static str, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w(px(SWATCH_WIDTH))
        .child(
            div()
                .flex_none()
                .size(t.spacing.row_height * 0.75)
                .rounded(t.radii.sm)
                .border_1()
                .border_color(t.colors.line)
                .bg(color),
        )
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.tiny)
                .text_color(t.colors.muted)
                .child(name),
        )
}

fn swatches(t: &Theme) -> Div {
    let grid = div().flex().flex_wrap().gap(t.spacing.sm).children(
        t.surfaces
            .entries()
            .into_iter()
            .map(|(name, color)| swatch(t, name, color)),
    );
    section(t, "COLOURS", grid)
}

fn figure(t: &Theme, label: String) -> Div {
    div()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(label)
}

fn sizes(t: &Theme) -> Div {
    let r = t.radii;
    let m = t.motion;
    let body = div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(figure(
            t,
            format!(
                "radius card {} · pill {} · keycap {} · sheet {} · job card {}",
                f32::from(r.card),
                f32::from(r.pill),
                f32::from(r.keycap),
                f32::from(r.sheet),
                f32::from(r.job_card)
            ),
        ))
        .child(figure(
            t,
            format!("motion: fade {} ms, no travel", m.fade.as_millis()),
        ))
        .child(figure(
            t,
            "shadows card, card_active, job_card, pill, keycap, menu, chip, sheet".to_owned(),
        ));
    section(t, "RADII, MOTION, SHADOWS", body)
}

pub fn render(page: Page, t: &Theme) -> Div {
    if page == Page::Palettes {
        return palettes_page::render(t);
    }
    let tokens = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(SWATCH_WIDTH * 2.) + t.spacing.sm + t.spacing.xxl * 2.)
        .gap(t.spacing.xxl)
        .p(t.spacing.xxl)
        .child(swatches(t))
        .child(sizes(t));
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .child(tokens)
        .child(tokens_sample::render(t))
}

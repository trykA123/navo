use rusty_ui::prelude::*;
use rusty_ui::tokens::Theme;

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
                "radius card {} · pill {} · keycap {} · sheet {}",
                f32::from(r.card),
                f32::from(r.pill),
                f32::from(r.keycap),
                f32::from(r.sheet)
            ),
        ))
        .child(figure(
            t,
            format!(
                "motion fast {} · base {} · slow {} ms",
                m.fast.as_millis(),
                m.base.as_millis(),
                m.slow.as_millis()
            ),
        ))
        .child(figure(t, "reduced motion: fade only, no travel".to_owned()))
        .child(figure(
            t,
            format!(
                "shadow layers card {} · job card {}",
                t.shadows.card.len(),
                t.shadows.job_card.len()
            ),
        ));
    section(t, "RADII, MOTION, SHADOWS", body)
}

pub fn render(t: &Theme) -> Div {
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

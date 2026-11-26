//! GPUI icon element and theme-aware colour helpers for resolved button bar icons.

use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ObjectFit, ParentElement, Pixels, Rgba, Styled,
    StyledImage, Window, div, img, px, svg as svg_element,
};
use navo_icons::spec::{BrandColour, Shape, Tone};

use super::{BarIcon, raster_cache};
use crate::tokens::{self, Colors, contrast_ratio};

const BRAND_CONTRAST_MIN: f32 = 3.0;
const SOFT_FILL_OPACITY: f32 = 0.16;
const OUTLINE_BORDER_PX: f32 = 1.5;
const GLYPH_DOT_PX: f32 = 5.0;

/// Maps each of the 12 icon tones one-to-one to the matching theme `Colors` token.
#[must_use]
pub(crate) fn tone_colour(tone: Tone, colors: &Colors) -> Hsla {
    match tone {
        Tone::Accent => colors.accent,
        Tone::Ok => colors.ok,
        Tone::Warn => colors.warn,
        Tone::Folder => colors.folder,
        Tone::KindCode => colors.kind_code,
        Tone::KindAuto => colors.kind_auto,
        Tone::KindImage => colors.kind_image,
        Tone::KindArchive => colors.kind_archive,
        Tone::KindDoc => colors.kind_doc,
        Tone::KindApp => colors.kind_app,
        Tone::Muted => colors.muted,
        Tone::Text => colors.text,
    }
}

/// Picks the brand colour when `BrandColour::Brand` meets 3.0 contrast on `colors.rail`.
#[must_use]
pub(crate) fn brand_or_theme(brand: Option<[u8; 3]>, colour: BrandColour, colors: &Colors) -> Hsla {
    if colour != BrandColour::Brand {
        return colors.text;
    }
    let Some([r, g, b]) = brand else {
        return colors.text;
    };
    let brand_hsla = rgb_bytes_to_hsla([r, g, b]);
    if contrast_ratio(brand_hsla, colors.rail) >= BRAND_CONTRAST_MIN {
        brand_hsla
    } else {
        colors.text
    }
}

/// Computes `(fill, text)` colours for a letter badge given its shape, tone, and theme palette.
#[must_use]
pub(crate) fn letter_colours(shape: Shape, tone: Hsla, colors: &Colors) -> (Option<Hsla>, Hsla) {
    match shape {
        Shape::Solid | Shape::Circle => {
            let text =
                if contrast_ratio(colors.background, tone) >= contrast_ratio(colors.text, tone) {
                    colors.background
                } else {
                    colors.text
                };
            (Some(tone), text)
        }
        Shape::Soft => (Some(tone.opacity(SOFT_FILL_OPACITY)), tone),
        Shape::Outline | Shape::None => (None, tone),
    }
}

/// Builds a GPUI element of `side` x `side` for any resolved button bar icon variant.
#[must_use]
pub(crate) fn icon_element(icon: &BarIcon, side: Pixels, cx: &App) -> AnyElement {
    let t = tokens::current(cx);
    match icon {
        BarIcon::Mono { svg, tone, brand } => {
            let color = match (tone, brand) {
                (Some(tone), _) => tone_colour(*tone, &t.colors),
                (None, Some(rgb)) => brand_or_theme(Some(*rgb), BrandColour::Brand, &t.colors),
                (None, None) => t.colors.text,
            };
            svg_element()
                .data(svg)
                .size(side)
                .flex_none()
                .text_color(color)
                .into_any_element()
        }
        BarIcon::Colour { svg } => {
            let bytes = Arc::clone(svg);
            img(move |window: &mut Window, cx: &mut App| {
                Some(raster_cache::svg_image(
                    &bytes,
                    side,
                    window.scale_factor(),
                    cx,
                ))
            })
            .size(side)
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element()
        }
        BarIcon::Raster { png } => {
            let bytes = Arc::clone(png);
            img(move |window: &mut Window, cx: &mut App| {
                Some(raster_cache::png_image(
                    &bytes,
                    side,
                    window.scale_factor(),
                    cx,
                ))
            })
            .size(side)
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element()
        }
        BarIcon::Letters {
            letters,
            shape,
            tone,
        } => letters_element(letters, *shape, *tone, side, t),
        BarIcon::Glyph { svg, tone, dot } => glyph_element(svg, *tone, *dot, side, t),
        BarIcon::Missing { dashed } => missing_element(*dashed, side, t),
    }
}

fn letters_element(
    letters: &str,
    shape: Shape,
    tone: Tone,
    side: Pixels,
    t: &tokens::Theme,
) -> AnyElement {
    let tone_hsla = tone_colour(tone, &t.colors);
    let (fill, text_color) = letter_colours(shape, tone_hsla, &t.colors);
    let radius = match shape {
        Shape::Circle => t.radii.full,
        _ => t.radii.sm,
    };
    let text_size = if letters.chars().count() > 1 {
        px((side.as_f32() * 0.56).round().max(8.0))
    } else {
        px((side.as_f32() * 0.68).round().max(9.0))
    };

    div()
        .size(side)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius)
        .when_some(fill, |el, bg| el.bg(bg))
        .when(shape == Shape::Outline, |el| {
            el.border(px(OUTLINE_BORDER_PX)).border_color(tone_hsla)
        })
        .text_size(text_size)
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(side)
        .text_color(text_color)
        .child(letters.to_owned())
        .into_any_element()
}

fn glyph_element(
    svg: &[u8],
    tone: Tone,
    dot: Option<Tone>,
    side: Pixels,
    t: &tokens::Theme,
) -> AnyElement {
    let body = svg_element()
        .data(svg)
        .size(side)
        .flex_none()
        .text_color(tone_colour(tone, &t.colors));
    div()
        .relative()
        .size(side)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(body)
        .when_some(dot, |el, dot_tone| {
            el.child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(GLYPH_DOT_PX))
                    .rounded(t.radii.full)
                    .bg(tone_colour(dot_tone, &t.colors)),
            )
        })
        .into_any_element()
}

fn missing_element(dashed: bool, side: Pixels, t: &tokens::Theme) -> AnyElement {
    let box_el = div().size(side).flex_none();
    if dashed {
        box_el
            .rounded(t.radii.sm)
            .border_1()
            .border_dashed()
            .border_color(t.colors.faint)
            .into_any_element()
    } else {
        box_el.into_any_element()
    }
}

fn rgb_bytes_to_hsla([r, g, b]: [u8; 3]) -> Hsla {
    let mut rgba = Rgba::default().alpha(1.0);
    rgba.r = f32::from(r) / 255.0;
    rgba.g = f32::from(g) / 255.0;
    rgba.b = f32::from(b) / 255.0;
    Hsla::from(rgba)
}

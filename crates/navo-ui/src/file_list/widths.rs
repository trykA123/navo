use std::collections::HashMap;

use gpui_kit::{
    App, Font, FontFeatures, FontWeight, Global, Hsla, Pixels, SharedString, TextRun, Window, px,
};

use super::SortBy;
use super::header::label;
use crate::tokens::Theme;

const DATE_SHAPE: &str = "0000-00-00 00:00";
const SIZE_SHAPES: [&str; 9] = [
    "0000 B",
    "0000.0 KB",
    "0000.0 MB",
    "0000.0 GB",
    "0000.0 TB",
    "0000.0 PB",
    "0000.0 EB",
    "<DIR>",
    "<LNK>",
];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
// Multiples of spacing.xxl: 40, 120 and 320 px at 100 % scale.
const MIN_COLUMN: f32 = 2.5;
const MIN_NAME: f32 = 7.5;
const MAX_COLUMN: f32 = 20.;
const EXT_COLUMN: f32 = 3.;
const ARROW: f32 = 0.25;
const SLACK: f32 = 1.;
pub(super) const FIXED: [SortBy; 3] = [SortBy::Ext, SortBy::Size, SortBy::Date];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UserWidths {
    pub ext: Option<Pixels>,
    pub size: Option<Pixels>,
    pub date: Option<Pixels>,
}

impl UserWidths {
    fn get(self, by: SortBy) -> Option<Pixels> {
        match by {
            SortBy::Ext => self.ext,
            SortBy::Size => self.size,
            SortBy::Date => self.date,
            SortBy::Name => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ColumnWidths {
    ext: Pixels,
    size: Pixels,
    date: Pixels,
}

impl ColumnWidths {
    pub(super) fn fallback(t: &Theme) -> Self {
        Self {
            ext: t.spacing.xxl * EXT_COLUMN,
            size: t.spacing.size_column,
            date: t.spacing.date_column + t.spacing.xl,
        }
    }

    pub(super) fn resolve(user: UserWidths, t: &Theme, window: &Window, cx: &mut App) -> Self {
        FIXED
            .into_iter()
            .fold(natural(t, window, cx), |widths, by| match user.get(by) {
                Some(width) => widths.with(by, clamp_width(t, width)),
                None => widths,
            })
    }

    pub(super) fn of(self, by: SortBy) -> Option<Pixels> {
        match by {
            SortBy::Ext => Some(self.ext),
            SortBy::Size => Some(self.size),
            SortBy::Date => Some(self.date),
            SortBy::Name => None,
        }
    }

    #[must_use]
    pub(super) fn with(mut self, by: SortBy, width: Pixels) -> Self {
        match by {
            SortBy::Ext => self.ext = width,
            SortBy::Size => self.size = width,
            SortBy::Date => self.date = width,
            SortBy::Name => {}
        }
        self
    }
}

pub(super) fn min_width(t: &Theme) -> Pixels {
    t.spacing.xxl * MIN_COLUMN
}

pub(super) fn min_name(t: &Theme) -> Pixels {
    t.spacing.xxl * MIN_NAME
}

pub(super) fn clamp_width(t: &Theme, width: Pixels) -> Pixels {
    width.min(t.spacing.xxl * MAX_COLUMN).max(min_width(t))
}

type MeasureKey = (SharedString, SharedString, u32, u32, FontFeatures);

#[derive(Default)]
struct Measured(HashMap<MeasureKey, (Pixels, Pixels)>);

impl Global for Measured {}

fn natural(t: &Theme, window: &Window, cx: &mut App) -> ColumnWidths {
    let typography = &t.typography;
    let key = (
        typography.numbers_font.clone(),
        typography.ui_font.clone(),
        f32::from(typography.small).to_bits(),
        f32::from(t.spacing.xs).to_bits(),
        typography.numbers_font_features.clone(),
    );
    let found = cx
        .try_global::<Measured>()
        .and_then(|measured| measured.0.get(&key).copied());
    let (size, date) = found.unwrap_or_else(|| {
        let measured = measure_numbers(t, window);
        cx.default_global::<Measured>().0.insert(key, measured);
        measured
    });
    let fallback = ColumnWidths::fallback(t);
    let usable = |width: Pixels, or: Pixels| if width > px(0.) { width } else { or };
    ColumnWidths {
        ext: fallback.ext,
        size: usable(size, fallback.size),
        date: usable(date, fallback.date),
    }
}

fn measure_numbers(t: &Theme, window: &Window) -> (Pixels, Pixels) {
    let font = numbers_font(t);
    let size = t.typography.small;
    let widest = DIGITS
        .into_iter()
        .map(|digit| (text_width(window, digit, &font, size), digit))
        .fold(
            (px(0.), "0"),
            |best, next| if next.0 > best.0 { next } else { best },
        )
        .1;
    let shaped = |shape: &str| shape.replace('0', widest);
    let size_texts = SIZE_SHAPES.map(shaped);
    (
        fit_texts(
            t,
            window,
            SortBy::Size,
            size_texts.iter().map(String::as_str),
        ),
        fit_texts(
            t,
            window,
            SortBy::Date,
            std::iter::once(shaped(DATE_SHAPE).as_str()),
        ),
    )
}

pub(super) fn fit_texts<'a>(
    t: &Theme,
    window: &Window,
    by: SortBy,
    texts: impl IntoIterator<Item = &'a str>,
) -> Pixels {
    let (font, size) = match by {
        SortBy::Size | SortBy::Date => (numbers_font(t), t.typography.small),
        SortBy::Ext | SortBy::Name => (
            semibold(gpui_kit::font(t.typography.ui_font.clone())),
            t.typography.body,
        ),
    };
    let content = texts
        .into_iter()
        .map(|text| text_width(window, text, &font, size))
        .fold(px(0.), Pixels::max);
    let width = content.max(header_room(t, window, by));
    if !f32::from(width).is_finite() || width <= px(0.) {
        return px(0.);
    }
    px(f32::from(width).ceil() + SLACK)
}

fn header_room(t: &Theme, window: &Window, by: SortBy) -> Pixels {
    let font = semibold(gpui_kit::font(t.typography.ui_font.clone()));
    let text = text_width(window, label(by), &font, t.typography.small);
    text + t.spacing.xs + t.spacing.xxl * ARROW
}

fn numbers_font(t: &Theme) -> Font {
    let mut font = gpui_kit::font(t.typography.numbers_font.clone());
    font.features = t.typography.numbers_font_features.clone();
    semibold(font)
}

fn semibold(mut font: Font) -> Font {
    font.weight = FontWeight::SEMIBOLD;
    font
}

fn text_width(window: &Window, text: &str, font: &Font, size: Pixels) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .layout_line(text, size, &[run], None)
        .width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    fn shape(text: &str) -> String {
        text.chars()
            .map(|ch| if ch.is_ascii_digit() { '0' } else { ch })
            .collect()
    }

    #[test]
    fn the_date_shape_matches_the_date_format() {
        assert_eq!(shape(&format::date(1_791_920_400_000)), DATE_SHAPE);
    }

    #[test]
    fn every_size_text_has_a_shape_at_least_as_long_with_its_unit() {
        for bytes in [
            0,
            1023,
            1024,
            1_048_575,
            1_073_689_395,
            4_000_000_000,
            u64::MAX,
        ] {
            let text = shape(&format::size(bytes));
            let unit = text.rsplit(' ').next();
            assert!(
                SIZE_SHAPES
                    .iter()
                    .any(|shape| shape.rsplit(' ').next() == unit && shape.len() >= text.len()),
                "{text}"
            );
        }
    }
}

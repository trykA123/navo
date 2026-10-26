use std::ops::Range;

use super::timing::Frame;

const WIDTH: f32 = 320.0;
const HEIGHT: f32 = 240.0;
const RADIUS: f32 = 12.0;
const QUARTER: f32 = 4.0;
const STATUS_TOP: f32 = 184.0;
const STATUS_INSET: f32 = 16.0;

// Open sea and icon variant A colours from D78, converted from OKLCH to sRGB.
const TILE: u32 = 0x08_1115;
const GLYPH: u32 = 0xE2_EDF2;
const BAR: u32 = 0x50_C2DF;

const TILE_MASK: &[u8] = include_bytes!("../../../../assets/brand/splash/tile.pgm");
const GLYPH_MASK: &[u8] = include_bytes!("../../../../assets/brand/splash/glyph.pgm");
const BAR_MASK: &[u8] = include_bytes!("../../../../assets/brand/splash/bar.pgm");
const WORDMARK_MASK: &[u8] = include_bytes!("../../../../assets/brand/splash/wordmark.pgm");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Look {
    surface: u32,
    border: u32,
    text: u32,
    pub(super) muted: u32,
}

pub(super) const LIGHT: Look = Look {
    surface: 0xF5_FBFE,
    border: 0xD2_DDE2,
    text: 0x14_1C20,
    muted: 0x56_6267,
};

pub(super) const DARK: Look = Look {
    surface: 0x15_1D21,
    border: 0x2E_373B,
    text: 0xE1_EDF2,
    muted: 0x9A_A7AD,
};

struct Mask {
    x: i32,
    y: i32,
    width: usize,
    height: usize,
    data: &'static [u8],
}

struct Coverage {
    x: usize,
    y: usize,
    width: usize,
    values: Vec<f32>,
}

pub(super) struct Canvas {
    width: usize,
    height: usize,
    base: Vec<u32>,
    pixels: Vec<u32>,
    bar: Coverage,
    scale: f32,
}

impl Canvas {
    pub(super) fn new(scale: f32, look: Look) -> Option<Self> {
        let width = (WIDTH * scale).round() as usize;
        let height = (HEIGHT * scale).round() as usize;
        let mut base = shape(width, height, scale, look);
        for (mask, colour) in [
            (TILE_MASK, TILE),
            (GLYPH_MASK, GLYPH),
            (WORDMARK_MASK, look.text),
        ] {
            paint(
                &mut base,
                width,
                &cover(&parse(mask)?, scale),
                (colour, 1.0),
            );
        }
        let bar = cover(&parse(BAR_MASK)?, scale);
        let pixels = base.clone();
        let mut canvas = Self {
            width,
            height,
            base,
            pixels,
            bar,
            scale,
        };
        canvas.set_bar(1.0);
        Some(canvas)
    }

    pub(super) fn set_bar(&mut self, opacity: f32) {
        let Coverage { x, y, width, .. } = self.bar;
        for row in 0..self.bar.values.len() / width.max(1) {
            let start = (y + row) * self.width + x;
            if let (Some(to), Some(from)) = (
                self.pixels.get_mut(start..start + width),
                self.base.get(start..start + width),
            ) {
                to.copy_from_slice(from);
            }
        }
        paint(&mut self.pixels, self.width, &self.bar, (BAR, opacity));
    }

    pub(super) fn width(&self) -> usize {
        self.width
    }

    pub(super) fn height(&self) -> usize {
        self.height
    }

    pub(super) fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub(super) fn status(&self) -> [i32; 4] {
        let inset = (STATUS_INSET * self.scale).round() as i32;
        [
            inset,
            (STATUS_TOP * self.scale).round() as i32,
            self.width as i32 - inset,
            self.height as i32 - inset / 2,
        ]
    }

    pub(super) fn changed_rows(&self, before: Option<Frame>, now: Frame) -> Vec<Range<usize>> {
        let Some(before) = before else {
            return std::iter::once(0..self.height).collect();
        };
        let bar_rows = self.bar.values.len() / self.bar.width.max(1);
        let bar = self.bar.y..(self.bar.y + bar_rows).min(self.height);
        let status = (before.is_slow != now.is_slow).then(|| {
            let [_, top, _, bottom] = self.status();
            let clamp = |value: i32| usize::try_from(value).unwrap_or(0).min(self.height);
            clamp(top)..clamp(bottom)
        });
        std::iter::once(bar).chain(status).collect()
    }
}

pub(super) fn copy_rows(to: &mut [u32], from: &[u32], width: usize, rows: &[Range<usize>]) {
    for range in rows {
        let end = (range.end * width).min(to.len()).min(from.len());
        let span = (range.start * width).min(end)..end;
        if let (Some(to), Some(from)) = (to.get_mut(span.clone()), from.get(span)) {
            to.copy_from_slice(from);
        }
    }
}

pub(super) fn make_opaque(pixels: &mut [u32], width: usize, [left, top, right, bottom]: [i32; 4]) {
    let clamp = |value: i32, limit: usize| usize::try_from(value).unwrap_or(0).min(limit);
    let (left, right) = (clamp(left, width), clamp(right, width));
    let rows = pixels.len() / width.max(1);
    for row in clamp(top, rows)..clamp(bottom, rows) {
        for pixel in &mut pixels[row * width + left..row * width + right.max(left)] {
            *pixel |= 0xFF00_0000;
        }
    }
}

fn parse(bytes: &'static [u8]) -> Option<Mask> {
    let lines: Vec<&'static [u8]> = bytes.splitn(5, |byte| *byte == b'\n').collect();
    let [b"P5", place, size, b"255", data] = lines.as_slice() else {
        return None;
    };
    let numbers = |line: &[u8]| -> Option<Vec<i64>> {
        Some(
            std::str::from_utf8(line)
                .ok()?
                .split_whitespace()
                .filter_map(|word| word.parse().ok())
                .collect(),
        )
    };
    let (Some([x, y]), Some([width, height])) = (
        numbers(place).and_then(|found| <[i64; 2]>::try_from(found).ok()),
        numbers(size).and_then(|found| <[i64; 2]>::try_from(found).ok()),
    ) else {
        return None;
    };
    let mask = Mask {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        width: usize::try_from(width).ok()?,
        height: usize::try_from(height).ok()?,
        data,
    };
    (mask.data.len() == mask.width * mask.height && place.starts_with(b"# at ")).then_some(mask)
}

fn weights(device: usize, origin: i32, len: usize, unit: f32) -> Vec<(usize, f32)> {
    let from = device as f32 / unit - origin as f32;
    let to = from + 1.0 / unit;
    let first = from.floor().max(0.0) as usize;
    let last = (to.ceil().max(0.0) as usize).min(len);
    (first..last)
        .map(|cell| {
            let overlap = to.min(cell as f32 + 1.0) - from.max(cell as f32);
            (cell, overlap.max(0.0) * unit)
        })
        .collect()
}

fn cover(mask: &Mask, scale: f32) -> Coverage {
    let unit = scale / QUARTER;
    let span = |origin: i32, len: usize| {
        let first = (origin as f32 * unit).floor().max(0.0) as usize;
        let last = ((origin + len as i32) as f32 * unit).ceil().max(0.0) as usize;
        let weights = (first..last)
            .map(|device| weights(device, origin, len, unit))
            .collect::<Vec<_>>();
        (first, weights)
    };
    let (x, columns) = span(mask.x, mask.width);
    let (y, rows) = span(mask.y, mask.height);
    let mut values = Vec::with_capacity(columns.len() * rows.len());
    for row in &rows {
        for column in &columns {
            let sum: f32 = row
                .iter()
                .flat_map(|(my, wy)| {
                    column
                        .iter()
                        .map(move |(mx, wx)| wy * wx * f32::from(mask.data[my * mask.width + mx]))
                })
                .sum();
            values.push(sum / 255.0);
        }
    }
    Coverage {
        x,
        y,
        width: columns.len(),
        values,
    }
}

fn paint(pixels: &mut [u32], stride: usize, coverage: &Coverage, (colour, opacity): (u32, f32)) {
    let width = coverage.width.max(1);
    for (index, amount) in coverage.values.iter().enumerate() {
        let (x, y) = (coverage.x + index % width, coverage.y + index / width);
        if let Some(pixel) = pixels.get_mut(y * stride + x).filter(|_| x < stride) {
            *pixel = blend(*pixel, colour, amount * opacity);
        }
    }
}

fn blend(under: u32, colour: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let below = ((under >> shift) & 0xFF) as f32;
        let above = ((colour >> shift) & 0xFF) as f32;
        ((below + (above - below) * amount).round() as u32) << shift
    };
    (under & 0xFF00_0000) | channel(16) | channel(8) | channel(0)
}

fn rounded(x: f32, y: f32, half: (f32, f32), radius: f32) -> f32 {
    let qx = x.abs() - (half.0 - radius);
    let qy = y.abs() - (half.1 - radius);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    outside + qx.max(qy).min(0.0) - radius
}

fn shape(width: usize, height: usize, scale: f32, look: Look) -> Vec<u32> {
    let half = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = RADIUS * scale;
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let x = column as f32 + 0.5 - half.0;
            let y = row as f32 + 0.5 - half.1;
            let edge = rounded(x, y, half, radius);
            let alpha = (0.5 - edge).clamp(0.0, 1.0);
            let inside = (0.5 - edge - scale).clamp(0.0, 1.0);
            let colour = blend(look.border, look.surface, inside);
            pixels.push(premultiply(colour, alpha));
        }
    }
    pixels
}

fn premultiply(colour: u32, alpha: f32) -> u32 {
    let scaled = blend(colour, 0, 1.0 - alpha) & 0x00FF_FFFF;
    (((alpha * 255.0).round() as u32) << 24) | scaled
}

#[cfg(test)]
#[path = "canvas_tests.rs"]
mod tests;

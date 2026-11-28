//! GPUI terminal grid element: paints a [`navo_term::grid::Snapshot`] with a per-row
//! [`gpui_kit::ShapedLine`] cache, coalesced cell background runs, selection overlay, wide
//! character alignment, and cursor rendering.

mod cache;
mod grid;
mod palette;

#[cfg(test)]
mod tests;

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, SharedString, TextRun, Window, font,
    px,
};
use navo_term::grid::{Point as GridPoint, Selection};

pub use cache::RowCache;
pub use grid::{BlockKind, GridStyle, RowBlock, TerminalGrid};
pub use palette::{BgRun, CellColors, coalesce_bg_runs, resolve_cell_colors};

/// Full period of the terminal cursor blink cycle (1.06 s, strictly under 1 Hz).
pub const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(1060);
/// Half-period of the terminal cursor blink cycle (530 ms on, 530 ms off).
pub const CURSOR_BLINK_HALF_PERIOD: Duration = Duration::from_millis(530);

/// Fallback cell width ratio when the platform text system reports a zero advance.
const FALLBACK_WIDTH_RATIO: f32 = 0.6;

/// Measured monospace cell dimensions in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMetrics {
    pub width: Pixels,
    pub height: Pixels,
}

/// OpenType features for the terminal grid: contextual alternates (`calt`) and standard
/// ligatures (`liga`) are disabled so `!=` and `->` always occupy exact cell columns.
pub fn terminal_font_features() -> FontFeatures {
    FontFeatures(Arc::new(vec![("calt".into(), 0), ("liga".into(), 0)]))
}

/// Builds a monospace [`Font`] with ligatures disabled and the requested weight and slant.
pub(crate) fn terminal_font(family: SharedString, bold: bool, italic: bool) -> Font {
    let mut face = font(family);
    face.features = terminal_font_features();
    face.weight = if bold {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::NORMAL
    };
    if italic {
        face.style = FontStyle::Italic;
    }
    face
}

/// Computes `(cols, rows)` fitting inside a viewport of `width` × `height` pixels.
///
/// Non-positive or non-finite inputs yield `0` on the affected axis; both axes are clamped to
/// `i16::MAX` (ConPTY's upper limit).
pub fn grid_dimensions(
    width: Pixels,
    height: Pixels,
    cell_width: Pixels,
    cell_height: Pixels,
) -> (u16, u16) {
    let cols = fit_cells(width, cell_width);
    let rows = fit_cells(height, cell_height);
    if cols == 0 && (f32::from(cell_width) <= 0.0 || !f32::from(cell_width).is_finite()) {
        return (0, 0);
    }
    if rows == 0 && (f32::from(cell_height) <= 0.0 || !f32::from(cell_height).is_finite()) {
        return (0, 0);
    }
    (cols, rows)
}

fn fit_cells(span: Pixels, cell: Pixels) -> u16 {
    let span = f32::from(span);
    let cell = f32::from(cell);
    if !span.is_finite() || !cell.is_finite() || span <= 0.0 || cell <= 0.0 {
        return 0;
    }
    let count = (span / cell).floor().clamp(0.0, f32::from(i16::MAX));
    count as u16
}

/// Measures one monospace cell for `font_family` at `font_size` and `line_height_multiplier`.
pub fn measure_cell(
    window: &Window,
    font_family: &SharedString,
    font_size: Pixels,
    line_height_multiplier: f32,
) -> CellMetrics {
    let run = TextRun {
        len: 1,
        font: terminal_font(font_family.clone(), false, false),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let measured = window
        .text_system()
        .layout_line("M", font_size, &[run], None)
        .width;
    let raw_width = f32::from(measured);
    let width = if raw_width.is_finite() && raw_width > 0.0 {
        measured
    } else {
        px((f32::from(font_size) * FALLBACK_WIDTH_RATIO).max(1.0))
    };
    let mult = if line_height_multiplier.is_finite() && line_height_multiplier > 0.0 {
        line_height_multiplier
    } else {
        1.4
    };
    let height = px((f32::from(font_size) * mult).ceil().max(1.0));
    CellMetrics { width, height }
}

/// Returns whether the cursor is visible (`opacity = 1.0`) at `elapsed` time into the blink
/// cycle. Blinking is strictly off when `!blink_enabled` or `reduce_motion` is active.
pub fn cursor_visible(elapsed: Duration, blink_enabled: bool, reduce_motion: bool) -> bool {
    if !blink_enabled || reduce_motion {
        return true;
    }
    let phase = elapsed.as_millis() % CURSOR_BLINK_PERIOD.as_millis();
    phase < CURSOR_BLINK_HALF_PERIOD.as_millis()
}

/// Returns the half-open column range selected on visible viewport `row` (0 = top visible row).
pub fn selection_columns(
    selection: Selection,
    row: usize,
    cols: usize,
    display_offset: usize,
) -> Option<Range<usize>> {
    if cols == 0 {
        return None;
    }
    let (start, end) = ordered_points(selection.start, selection.end);
    let line = viewport_line(row, display_offset);
    if line < start.line || line > end.line {
        return None;
    }
    let (start_col, end_col) = if selection.block {
        let left = start.col.min(end.col).min(cols);
        let right = start.col.max(end.col).saturating_add(1).min(cols);
        (left, right)
    } else {
        let left = if line == start.line { start.col } else { 0 }.min(cols);
        let right = if line == end.line {
            end.col.saturating_add(1)
        } else {
            cols
        }
        .min(cols);
        (left, right)
    };
    (start_col < end_col).then_some(start_col..end_col)
}

fn ordered_points(a: GridPoint, b: GridPoint) -> (GridPoint, GridPoint) {
    if (a.line, a.col) <= (b.line, b.col) {
        (a, b)
    } else {
        (b, a)
    }
}

fn viewport_line(row: usize, display_offset: usize) -> i32 {
    let row = i32::try_from(row).unwrap_or(i32::MAX);
    let offset = i32::try_from(display_offset).unwrap_or(i32::MAX);
    row.saturating_sub(offset)
}

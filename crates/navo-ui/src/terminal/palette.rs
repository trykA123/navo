//! Resolves `navo-term` cell colours against `TerminalColors` tokens and coalesces adjacent cell
//! backgrounds into horizontal runs.

use gpui_kit::{Hsla, Rgba};
use navo_term::grid::{Cell, Color, ThemeRgb, Width};

use crate::tokens::{TerminalColors, contrast_ratio};

/// Converts `TerminalColors` into sRGB triples for OSC 10/11/12 query replies.
#[must_use]
pub fn theme_rgb(colors: &TerminalColors) -> ThemeRgb {
    ThemeRgb {
        fg: hsla_to_rgb(colors.text),
        bg: hsla_to_rgb(colors.background),
        cursor: hsla_to_rgb(colors.cursor),
    }
}

fn hsla_to_rgb(color: Hsla) -> (u8, u8, u8) {
    let rgba: Rgba = color.into();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    (channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

/// Opacity applied to explicit ANSI/RGB and inverse foregrounds when SGR 2 (`dim`) is active.
const DIM_OPACITY: f32 = 0.66;
/// Minimum WCAG AA contrast ratio required for normal text.
const TEXT_AA_CONTRAST: f32 = 4.5;

/// Resolved foreground and optional non-default background colour for one grid cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellColors {
    pub fg: Hsla,
    pub bg: Option<Hsla>,
}

/// One horizontal run of adjacent cells sharing the same non-default background colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgRun {
    pub start_col: usize,
    pub len_cols: usize,
    pub color: Hsla,
}

/// Resolves `cell`'s foreground and optional non-default background from `colors`.
pub fn resolve_cell_colors(cell: &Cell, colors: &TerminalColors) -> CellColors {
    let raw_fg = resolve_color(cell.fg, colors.text, colors);
    let raw_bg = resolve_color(cell.bg, colors.background, colors);
    let (base_fg, bg) = if cell.inverse {
        (raw_bg, Some(raw_fg))
    } else {
        (raw_fg, (cell.bg != Color::Default).then_some(raw_bg))
    };
    let fg = if cell.dim {
        dim_foreground(cell.fg, cell.inverse, base_fg, colors)
    } else {
        base_fg
    };
    CellColors { fg, bg }
}

/// Resolves `cell`'s foreground when painted over the opaque `colors.selection` highlight,
/// guaranteeing at least WCAG AA (`4.5:1`) contrast against `colors.selection`.
pub fn resolve_selected_fg(cell: &Cell, colors: &TerminalColors) -> Hsla {
    let fg = resolve_cell_colors(cell, colors).fg;
    ensure_contrast_on(fg, colors.selection, colors)
}

/// Returns the foreground colour used to draw a character inside a solid `CursorShape::Block`,
/// guaranteeing at least WCAG AA (`4.5:1`) contrast against `colors.cursor`.
pub fn block_cursor_glyph_color(colors: &TerminalColors) -> Hsla {
    ensure_contrast_on(colors.background, colors.cursor, colors)
}

/// Coalesces adjacent cells with identical non-default background colours into horizontal runs.
///
/// A [`Width::Spacer`] cell with a default background inherits its preceding [`Width::Wide`]
/// cell's background so two-column glyphs always receive a contiguous two-cell fill.
pub fn coalesce_bg_runs(row: &[Cell], colors: &TerminalColors) -> Vec<BgRun> {
    let mut runs: Vec<BgRun> = Vec::new();
    let mut prev_wide_bg: Option<Hsla> = None;

    for (col, cell) in row.iter().enumerate() {
        let resolved = resolve_cell_colors(cell, colors).bg;
        let bg = match cell.width {
            Width::Wide => {
                prev_wide_bg = resolved;
                resolved
            }
            Width::Spacer => {
                let inherited = resolved.or(prev_wide_bg);
                prev_wide_bg = None;
                inherited
            }
            Width::Single => {
                prev_wide_bg = None;
                resolved
            }
        };
        let Some(color) = bg else {
            continue;
        };
        if let Some(last) = runs.last_mut()
            && last.color == color
            && last.start_col + last.len_cols == col
        {
            last.len_cols += 1;
        } else {
            runs.push(BgRun {
                start_col: col,
                len_cols: 1,
                color,
            });
        }
    }
    runs
}

fn resolve_color(color: Color, default: Hsla, palette: &TerminalColors) -> Hsla {
    match color {
        Color::Default => default,
        Color::Ansi(index) => palette.ansi_color(index),
        Color::Rgb(r, g, b) => TerminalColors::rgb_color(r, g, b),
    }
}

fn dim_foreground(color: Color, inverse: bool, resolved: Hsla, palette: &TerminalColors) -> Hsla {
    if !inverse && color == Color::Default {
        palette.muted
    } else {
        resolved.opacity(DIM_OPACITY)
    }
}

fn ensure_contrast_on(preferred: Hsla, surface: Hsla, colors: &TerminalColors) -> Hsla {
    if contrast_ratio(preferred, surface) >= TEXT_AA_CONTRAST {
        return preferred;
    }
    let theme_best = higher_contrast(colors.text, colors.background, surface);
    if contrast_ratio(theme_best, surface) >= TEXT_AA_CONTRAST {
        return theme_best;
    }
    higher_contrast(
        TerminalColors::rgb_color(0, 0, 0),
        TerminalColors::rgb_color(255, 255, 255),
        surface,
    )
}

fn higher_contrast(a: Hsla, b: Hsla, surface: Hsla) -> Hsla {
    if contrast_ratio(a, surface) >= contrast_ratio(b, surface) {
        a
    } else {
        b
    }
}

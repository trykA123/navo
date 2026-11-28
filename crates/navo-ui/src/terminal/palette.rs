//! Resolves `navo-term` cell colours against `TerminalColors` tokens and coalesces adjacent cell
//! backgrounds into horizontal runs.

use gpui_kit::Hsla;
use navo_term::grid::{Cell, Color, Width};

use crate::tokens::TerminalColors;

/// Opacity applied to explicit ANSI/RGB foregrounds when SGR 2 (`dim`) is active.
const DIM_OPACITY: f32 = 0.66;

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
    if cell.inverse {
        return CellColors {
            fg: raw_bg,
            bg: Some(raw_fg),
        };
    }
    let fg = if cell.dim {
        dim_foreground(cell.fg, raw_fg, colors)
    } else {
        raw_fg
    };
    let bg = (cell.bg != Color::Default).then_some(raw_bg);
    CellColors { fg, bg }
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

fn dim_foreground(color: Color, resolved: Hsla, palette: &TerminalColors) -> Hsla {
    if color == Color::Default {
        palette.muted
    } else {
        resolved.opacity(DIM_OPACITY)
    }
}

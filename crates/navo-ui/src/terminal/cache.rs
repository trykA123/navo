//! Per-row shaped-line cache for [`super::TerminalGrid`]: re-shapes only damaged rows or on
//! font/size/theme/width invalidation.

use std::sync::{Arc, Weak};

#[cfg(test)]
use gpui_kit::Bounds;
use gpui_kit::{
    Hsla, Pixels, ShapedLine, SharedString, StrikethroughStyle, TextRun, UnderlineStyle, Window, px,
};
use navo_term::grid::{Cell, Snapshot, Width};

use super::palette::{BgRun, coalesce_bg_runs, resolve_cell_colors};
use super::terminal_font;
use crate::tokens::TerminalColors;

/// Stroke thickness for SGR underline and strikeout decorations.
const DECORATION_THICKNESS: f32 = 1.0;

/// Parameters whose change invalidates every cached row.
#[derive(Clone, PartialEq)]
pub(super) struct RenderKey {
    pub font_family: SharedString,
    pub font_size_bits: u32,
    pub line_height_bits: u32,
    pub cell_width_bits: u32,
    pub cell_height_bits: u32,
    pub colors: TerminalColors,
    pub viewport_width_bits: u32,
    pub display_offset: usize,
    pub row_count: usize,
    pub col_count: usize,
}

/// Inputs required to synchronize and shape rows for the current frame.
pub(super) struct ShapeContext<'a> {
    pub key: RenderKey,
    pub font_size: Pixels,
    pub cell_width: Pixels,
    pub window: &'a Window,
}

/// One wide character glyph shaped at its column offset so following single-width cells stay on
/// the monospace grid.
pub(super) struct WideGlyph {
    pub col: usize,
    pub line: ShapedLine,
}

/// Cached shaped output and background runs for a single visible row.
pub(super) struct CachedRow {
    pub line: Option<ShapedLine>,
    pub wide_glyphs: Vec<WideGlyph>,
    pub bg_runs: Vec<BgRun>,
}

/// Style attributes merged across adjacent cells within one [`TextRun`].
#[derive(Clone, Copy, PartialEq)]
struct RunStyle {
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
}

/// Persistent per-row shaping cache held by the terminal view across frames.
#[derive(Default)]
pub struct RowCache {
    key: Option<RenderKey>,
    last_snapshot: Option<Weak<Snapshot>>,
    rows: Vec<Option<CachedRow>>,
    #[cfg(test)]
    reshape_count: usize,
    #[cfg(test)]
    last_grid_dims: (u16, u16),
    #[cfg(test)]
    last_cursor_visible: Option<bool>,
    #[cfg(test)]
    last_cursor_bounds: Option<Bounds<Pixels>>,
}

impl RowCache {
    /// Creates an empty row cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of rows currently held in the cache.
    pub fn cached_row_count(&self) -> usize {
        self.rows.len()
    }

    /// Updates cached rows for `snapshot`, reshaping only damaged rows unless the render key
    /// changed (font, size, theme, viewport width, scroll offset, or grid dimensions).
    pub(super) fn sync(&mut self, snapshot: &Arc<Snapshot>, ctx: &ShapeContext<'_>) {
        if self.key.as_ref() != Some(&ctx.key) {
            self.rebuild_all(snapshot, ctx);
            return;
        }
        let snapshot_ref = Arc::downgrade(snapshot);
        if self
            .last_snapshot
            .as_ref()
            .is_some_and(|prev| prev.ptr_eq(&snapshot_ref))
        {
            return;
        }
        self.last_snapshot = Some(snapshot_ref);
        self.reshape_damaged(snapshot, ctx);
    }

    pub(super) fn row(&self, index: usize) -> Option<&CachedRow> {
        self.rows.get(index)?.as_ref()
    }

    fn rebuild_all(&mut self, snapshot: &Arc<Snapshot>, ctx: &ShapeContext<'_>) {
        self.key = Some(ctx.key.clone());
        self.last_snapshot = Some(Arc::downgrade(snapshot));
        self.rows.clear();
        self.rows.reserve(snapshot.rows.len());
        for cells in &snapshot.rows {
            let shaped = self.shape_row(cells, ctx);
            self.rows.push(Some(shaped));
        }
    }

    fn reshape_damaged(&mut self, snapshot: &Arc<Snapshot>, ctx: &ShapeContext<'_>) {
        let mut shaped_flags = vec![false; snapshot.rows.len()];
        for &row_u16 in &snapshot.damaged {
            let row_idx = usize::from(row_u16);
            if let Some(flag) = shaped_flags.get_mut(row_idx)
                && !*flag
            {
                *flag = true;
                let shaped = self.shape_row(&snapshot.rows[row_idx], ctx);
                self.rows[row_idx] = Some(shaped);
            }
        }
        for (row_idx, cells) in snapshot.rows.iter().enumerate() {
            if self.rows[row_idx].is_none() {
                let shaped = self.shape_row(cells, ctx);
                self.rows[row_idx] = Some(shaped);
            }
        }
    }

    fn shape_row(&mut self, cells: &[Cell], ctx: &ShapeContext<'_>) -> CachedRow {
        #[cfg(test)]
        {
            self.reshape_count += 1;
        }
        let bg_runs = coalesce_bg_runs(cells, &ctx.key.colors);
        let Some(last) = last_visible_cell(cells) else {
            return CachedRow {
                line: None,
                wide_glyphs: Vec::new(),
                bg_runs,
            };
        };
        let (text, runs, wide_glyphs) = build_row_runs(&cells[..=last], ctx);
        let line = if text.is_empty() {
            None
        } else {
            Some(ctx.window.text_system().shape_line(
                text.into(),
                ctx.font_size,
                &runs,
                Some(ctx.cell_width),
            ))
        };
        CachedRow {
            line,
            wide_glyphs,
            bg_runs,
        }
    }

    #[cfg(test)]
    pub(super) fn record_paint(
        &mut self,
        dims: (u16, u16),
        cursor_visible: Option<bool>,
        cursor_bounds: Option<Bounds<Pixels>>,
    ) {
        self.last_grid_dims = dims;
        self.last_cursor_visible = cursor_visible;
        self.last_cursor_bounds = cursor_bounds;
    }

    /// Total number of individual row reshapes performed by this cache.
    #[cfg(test)]
    pub fn reshape_count(&self) -> usize {
        self.reshape_count
    }

    /// Grid dimensions `(cols, rows)` computed during the latest prepaint.
    #[cfg(test)]
    pub fn last_grid_dims(&self) -> (u16, u16) {
        self.last_grid_dims
    }

    /// Whether the cursor was painted as visible during the latest frame (`None` if no cursor).
    #[cfg(test)]
    pub fn last_cursor_visible(&self) -> Option<bool> {
        self.last_cursor_visible
    }

    /// Pixel bounds of the cursor painted during the latest frame, if any.
    #[cfg(test)]
    pub fn last_cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_cursor_bounds
    }

    /// Column indices of cached wide glyphs on `row`.
    #[cfg(test)]
    pub fn wide_glyph_cols(&self, row: usize) -> Vec<usize> {
        self.row(row)
            .map(|cached| cached.wide_glyphs.iter().map(|glyph| glyph.col).collect())
            .unwrap_or_default()
    }
}

fn last_visible_cell(cells: &[Cell]) -> Option<usize> {
    let last = cells.iter().rposition(|cell| {
        cell.underline
            || cell.strikeout
            || cell.width == Width::Wide
            || (!cell.hidden && !is_blank_char(cell.c))
    })?;
    if cells[last].width == Width::Wide
        && cells
            .get(last + 1)
            .is_some_and(|c| c.width == Width::Spacer)
    {
        Some(last + 1)
    } else {
        Some(last)
    }
}

fn build_row_runs(
    cells: &[Cell],
    ctx: &ShapeContext<'_>,
) -> (String, Vec<TextRun>, Vec<WideGlyph>) {
    let mut text = String::with_capacity(cells.len());
    let mut runs: Vec<TextRun> = Vec::new();
    let mut wide_glyphs: Vec<WideGlyph> = Vec::new();
    let mut prev_style: Option<RunStyle> = None;
    let mut wide_decor: Option<RunStyle> = None;

    for (col, cell) in cells.iter().enumerate() {
        let fg = resolve_cell_colors(cell, &ctx.key.colors).fg;
        let mut style = RunStyle {
            fg,
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            strikeout: cell.strikeout,
        };
        let emitted = match cell.width {
            Width::Wide => {
                wide_decor = Some(style);
                if !cell.hidden && !is_blank_char(cell.c) {
                    wide_glyphs.push(shape_wide_glyph(col, cell, fg, ctx));
                }
                ' '
            }
            Width::Spacer => {
                if let Some(inherited) = wide_decor.take() {
                    style = inherited;
                }
                ' '
            }
            Width::Single => {
                wide_decor = None;
                sanitize_char(cell.c, cell.hidden)
            }
        };
        text.push(emitted);
        append_run(
            &mut runs,
            &mut prev_style,
            style,
            emitted.len_utf8(),
            &ctx.key.font_family,
        );
    }
    (text, runs, wide_glyphs)
}

fn append_run(
    runs: &mut Vec<TextRun>,
    prev_style: &mut Option<RunStyle>,
    style: RunStyle,
    byte_len: usize,
    font_family: &SharedString,
) {
    if *prev_style == Some(style)
        && let Some(last) = runs.last_mut()
    {
        last.len += byte_len;
        return;
    }
    *prev_style = Some(style);
    runs.push(TextRun {
        len: byte_len,
        font: terminal_font(font_family.clone(), style.bold, style.italic),
        color: style.fg,
        background_color: None,
        underline: style.underline.then_some(UnderlineStyle {
            thickness: px(DECORATION_THICKNESS),
            color: Some(style.fg),
            wavy: false,
        }),
        strikethrough: style.strikeout.then_some(StrikethroughStyle {
            thickness: px(DECORATION_THICKNESS),
            color: Some(style.fg),
        }),
    });
}

fn shape_wide_glyph(col: usize, cell: &Cell, fg: Hsla, ctx: &ShapeContext<'_>) -> WideGlyph {
    let mut buf = [0u8; 4];
    let text = cell.c.encode_utf8(&mut buf);
    let run = TextRun {
        len: text.len(),
        font: terminal_font(ctx.key.font_family.clone(), cell.bold, cell.italic),
        color: fg,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line =
        ctx.window
            .text_system()
            .shape_line(SharedString::new(text), ctx.font_size, &[run], None);
    WideGlyph { col, line }
}

fn sanitize_char(c: char, hidden: bool) -> char {
    if hidden || is_blank_char(c) { ' ' } else { c }
}

fn is_blank_char(c: char) -> bool {
    matches!(c, ' ' | '\0' | '\n' | '\r' | '\t')
}

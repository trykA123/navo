//! Custom GPUI [`Element`] that paints a [`navo_term::grid::Snapshot`] from a [`RowCache`].

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    App, Bounds, ContentMask, Corners, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Style,
    TextAlign, Window, fill, point, px, relative, size,
};
use navo_term::grid::{CursorShape, Snapshot};

use super::cache::{RenderKey, RowCache, ShapeContext};
use super::cursor::{CursorEnv, PaintedCursor, paint_cursor, prepare_cursor};
use super::{CellMetrics, grid_dimensions, measure_cell, selection_columns};
use crate::tokens::{self, TerminalColors};

/// Width of the command-block left accent bar, in logical pixels.
const BLOCK_ACCENT_WIDTH: f32 = 2.0;

/// Visual state of a command block drawn behind a span of visible rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Completed command with zero exit status.
    Ok,
    /// Failed command with non-zero exit status.
    Failed,
    /// Currently running command.
    Running,
}

/// Optional command-block highlight anchored to visible row indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowBlock {
    pub start_row: u16,
    pub end_row: u16,
    pub kind: BlockKind,
    pub current: bool,
}

/// Visual parameters for painting a [`TerminalGrid`].
#[derive(Clone, Debug, PartialEq)]
pub struct GridStyle {
    pub font_family: Option<SharedString>,
    pub font_size: Pixels,
    pub line_height: f32,
    pub padding: Pixels,
    pub colors: Option<TerminalColors>,
    pub cursor_shape: Option<CursorShape>,
    pub blink_enabled: bool,
    pub blink_elapsed: Duration,
    pub bottom_radius: Pixels,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: px(13.0),
            line_height: 1.4,
            padding: px(8.0),
            colors: None,
            cursor_shape: None,
            blink_enabled: true,
            blink_elapsed: Duration::ZERO,
            bottom_radius: px(0.0),
        }
    }
}

/// State computed in [`Element::prepaint`] and consumed in [`Element::paint`].
pub struct GridPrepaint {
    colors: TerminalColors,
    metrics: CellMetrics,
    origin: Point<Pixels>,
    row_count: usize,
    selection_runs: Vec<(usize, Range<usize>)>,
    cursor: Option<PaintedCursor>,
}

/// Custom GPUI element that renders a terminal [`Snapshot`] using a persistent [`RowCache`].
pub struct TerminalGrid {
    snapshot: Arc<Snapshot>,
    cache: Entity<RowCache>,
    style: GridStyle,
    blocks: Vec<RowBlock>,
    height: Option<Pixels>,
}

impl TerminalGrid {
    /// Creates a terminal grid element for `snapshot` backed by `cache`.
    pub fn new(snapshot: Arc<Snapshot>, cache: Entity<RowCache>) -> Self {
        Self {
            snapshot,
            cache,
            style: GridStyle::default(),
            blocks: Vec::new(),
            height: None,
        }
    }

    /// Sets the grid typography, padding, palette and cursor style.
    #[must_use]
    pub fn style(mut self, style: GridStyle) -> Self {
        self.style = style;
        self
    }

    /// Attaches command-block highlights anchored to visible row spans.
    #[must_use]
    pub fn blocks(mut self, blocks: Vec<RowBlock>) -> Self {
        self.blocks = blocks;
        self
    }

    /// Sets an explicit element height instead of filling the parent.
    #[must_use]
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }
}

impl IntoElement for TerminalGrid {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TerminalGrid {
    type RequestLayoutState = ();
    type PrepaintState = GridPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = match self.height {
            Some(height) => height.into(),
            None => relative(1.0).into(),
        };
        style.min_size.width = px(0.0).into();
        style.min_size.height = px(0.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> GridPrepaint {
        crate::assets::load_mono_fonts(cx);
        let theme = tokens::current(cx);
        let colors = self.style.colors.unwrap_or(theme.terminal);
        let font_family = self
            .style
            .font_family
            .clone()
            .unwrap_or_else(|| theme.typography.mono_font.clone());
        let metrics = measure_cell(
            window,
            &font_family,
            self.style.font_size,
            self.style.line_height,
        );
        let pad = self.style.padding;
        let origin = point(bounds.origin.x + pad, bounds.origin.y + pad);
        let env = CursorEnv {
            font_family: &font_family,
            colors,
            origin,
            metrics,
            reduce_motion: cx.reduce_motion(),
        };
        self.sync_row_cache(&env, bounds.size.width, window, cx);
        let cursor = self
            .snapshot
            .cursor
            .map(|cursor| prepare_cursor(cursor, &self.snapshot, &self.style, &env, window));
        record_test_paint(&self.cache, bounds, pad, metrics, cursor.as_ref(), cx);
        let row_count = self.snapshot.rows.len();
        let col_count = self.snapshot.rows.first().map_or(0, Vec::len);
        GridPrepaint {
            colors,
            metrics,
            origin,
            row_count,
            selection_runs: collect_selections(&self.snapshot, row_count, col_count),
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut GridPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let bg = if self.style.bottom_radius > px(0.0) {
            fill(bounds, state.colors.background).corner_radii(Corners {
                top_left: px(0.0),
                top_right: px(0.0),
                bottom_right: self.style.bottom_radius,
                bottom_left: self.style.bottom_radius,
            })
        } else {
            fill(bounds, state.colors.background)
        };
        window.paint_quad(bg);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            paint_blocks(&self.blocks, bounds, state, window);
            paint_backgrounds(&self.cache, state, window, cx);
            paint_selections(state, window);
            paint_rows(&self.cache, state, window, cx);
            if let Some(cursor) = &state.cursor
                && cursor.visible
            {
                paint_cursor(
                    cursor,
                    state.colors.cursor,
                    state.metrics.height,
                    window,
                    cx,
                );
            }
        });
    }
}

impl TerminalGrid {
    fn sync_row_cache(
        &self,
        env: &CursorEnv<'_>,
        viewport_width: Pixels,
        window: &Window,
        cx: &mut App,
    ) {
        let key = RenderKey {
            font_family: env.font_family.clone(),
            font_size_bits: f32::from(self.style.font_size).to_bits(),
            line_height_bits: self.style.line_height.to_bits(),
            cell_width_bits: f32::from(env.metrics.width).to_bits(),
            cell_height_bits: f32::from(env.metrics.height).to_bits(),
            colors: env.colors,
            viewport_width_bits: f32::from(viewport_width).to_bits(),
            display_offset: self.snapshot.display_offset,
            row_count: self.snapshot.rows.len(),
            col_count: self.snapshot.rows.first().map_or(0, Vec::len),
        };
        let shape_ctx = ShapeContext {
            key,
            font_size: self.style.font_size,
            cell_width: env.metrics.width,
            window,
        };
        self.cache
            .update(cx, |cache, _| cache.sync(&self.snapshot, &shape_ctx));
    }
}

fn record_test_paint(
    cache: &Entity<RowCache>,
    bounds: Bounds<Pixels>,
    pad: Pixels,
    metrics: CellMetrics,
    cursor: Option<&PaintedCursor>,
    cx: &mut App,
) {
    let inner_w = (bounds.size.width - pad * 2.0).max(px(0.0));
    let inner_h = (bounds.size.height - pad * 2.0).max(px(0.0));
    let dims = grid_dimensions(inner_w, inner_h, metrics.width, metrics.height);
    #[cfg(test)]
    cache.update(cx, |cache, _| {
        cache.record_paint(dims, cursor.map(|c| c.visible), cursor.map(|c| c.bounds));
    });
    #[cfg(not(test))]
    let _ = (cache, dims, cursor, cx);
}

fn collect_selections(
    snapshot: &Snapshot,
    row_count: usize,
    col_count: usize,
) -> Vec<(usize, Range<usize>)> {
    let Some(selection) = snapshot.selection else {
        return Vec::new();
    };
    (0..row_count)
        .filter_map(|row| {
            let cols = selection_columns(selection, row, col_count, snapshot.display_offset)?;
            Some((row, cols))
        })
        .collect()
}

fn paint_blocks(
    blocks: &[RowBlock],
    bounds: Bounds<Pixels>,
    state: &GridPrepaint,
    window: &mut Window,
) {
    for block in blocks {
        let start = usize::from(block.start_row);
        let end = usize::from(block.end_row);
        if start >= state.row_count || end < start {
            continue;
        }
        let end = end.min(state.row_count - 1);
        let y = state.origin.y + state.metrics.height * (start as f32);
        let h = state.metrics.height * ((end - start + 1) as f32);
        if block.current {
            let band = Bounds::new(point(bounds.origin.x, y), size(bounds.size.width, h));
            window.paint_quad(fill(band, state.colors.block_current));
        }
        if block.current || block.kind == BlockKind::Failed {
            let bar_color = match block.kind {
                BlockKind::Failed => state.colors.mark_fail,
                BlockKind::Ok | BlockKind::Running => state.colors.mark_run,
            };
            let bar = Bounds::new(point(bounds.origin.x, y), size(px(BLOCK_ACCENT_WIDTH), h));
            window.paint_quad(fill(bar, bar_color));
        }
    }
}

fn paint_backgrounds(
    cache: &Entity<RowCache>,
    state: &GridPrepaint,
    window: &mut Window,
    cx: &App,
) {
    let cache = cache.read(cx);
    for row in 0..state.row_count {
        let Some(cached) = cache.row(row) else {
            continue;
        };
        let y = state.origin.y + state.metrics.height * (row as f32);
        for run in &cached.bg_runs {
            let x = state.origin.x + state.metrics.width * (run.start_col as f32);
            let w = state.metrics.width * (run.len_cols as f32);
            let rect = Bounds::new(point(x, y), size(w, state.metrics.height));
            window.paint_quad(fill(rect, run.color));
        }
    }
}

fn paint_selections(state: &GridPrepaint, window: &mut Window) {
    for (row, cols) in &state.selection_runs {
        let x = state.origin.x + state.metrics.width * (cols.start as f32);
        let y = state.origin.y + state.metrics.height * (*row as f32);
        let w = state.metrics.width * ((cols.end - cols.start) as f32);
        let rect = Bounds::new(point(x, y), size(w, state.metrics.height));
        window.paint_quad(fill(rect, state.colors.selection));
    }
}

fn paint_rows(cache: &Entity<RowCache>, state: &GridPrepaint, window: &mut Window, cx: &mut App) {
    let cache = cache.clone();
    for row in 0..state.row_count {
        let y = state.origin.y + state.metrics.height * (row as f32);
        let (line, wides) = {
            let guard = cache.read(cx);
            let Some(cached) = guard.row(row) else {
                continue;
            };
            let line = cached.line.clone();
            let wides: Vec<(usize, ShapedLine)> = cached
                .wide_glyphs
                .iter()
                .map(|glyph| (glyph.col, glyph.line.clone()))
                .collect();
            (line, wides)
        };
        if let Some(line) = line {
            let _ = line.paint(
                point(state.origin.x, y),
                state.metrics.height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        for (col, glyph) in wides {
            let x = state.origin.x + state.metrics.width * (col as f32);
            let _ = glyph.paint(
                point(x, y),
                state.metrics.height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}

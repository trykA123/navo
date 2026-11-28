//! Cursor geometry preparation and painting for [`super::TerminalGrid`].

use gpui_kit::{
    App, BorderStyle, Bounds, Hsla, Pixels, Point, ShapedLine, SharedString, TextAlign, TextRun,
    Window, fill, outline, point, px, size,
};
use navo_term::grid::{Cell, Cursor, CursorShape, Snapshot, Width};

use super::grid::GridStyle;
use super::palette::block_cursor_glyph_color;
use super::{CellMetrics, cursor_visible, terminal_font};
use crate::tokens::TerminalColors;

/// Width of the vertical bar cursor and thickness of the underline cursor, in logical pixels.
const CURSOR_STROKE: f32 = 2.0;

/// Grid layout and theme environment needed to position and shape the terminal cursor.
pub(super) struct CursorEnv<'a> {
    pub font_family: &'a SharedString,
    pub colors: TerminalColors,
    pub origin: Point<Pixels>,
    pub metrics: CellMetrics,
    pub reduce_motion: bool,
}

/// Prepainted cursor quad and optional high-contrast glyph redraw for a solid block cursor.
pub(super) struct PaintedCursor {
    pub shape: CursorShape,
    pub bounds: Bounds<Pixels>,
    pub visible: bool,
    block_glyph: Option<(Point<Pixels>, ShapedLine)>,
}

/// Computes the cursor's visibility, pixel bounds, and solid-block glyph overlay for this frame.
pub(super) fn prepare_cursor(
    cursor: Cursor,
    snapshot: &Snapshot,
    style: &GridStyle,
    env: &CursorEnv<'_>,
    window: &Window,
) -> PaintedCursor {
    let visible = cursor_visible(style.blink_elapsed, style.blink_enabled, env.reduce_motion);
    let shape = style.cursor_shape.unwrap_or(cursor.shape);
    let cell = snapshot
        .rows
        .get(usize::from(cursor.row))
        .and_then(|row| row.get(usize::from(cursor.col)));
    let span_cols = if cell.is_some_and(|c| c.width == Width::Wide) {
        2.0
    } else {
        1.0
    };
    let cell_origin = point(
        env.origin.x + env.metrics.width * f32::from(cursor.col),
        env.origin.y + env.metrics.height * f32::from(cursor.row),
    );
    let bounds = cursor_bounds(shape, cell_origin, env.metrics, span_cols);
    let block_glyph = (visible && shape == CursorShape::Block)
        .then(|| {
            cell.and_then(|c| {
                shape_cursor_glyph(c, env.font_family, style.font_size, env.colors, window)
            })
        })
        .flatten()
        .map(|line| (cell_origin, line));
    PaintedCursor {
        shape,
        bounds,
        visible,
        block_glyph,
    }
}

/// Paints the prepared cursor shape and any solid-block character overlay.
pub(super) fn paint_cursor(
    cursor: &PaintedCursor,
    cursor_color: Hsla,
    cell_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    match cursor.shape {
        CursorShape::HollowBlock => {
            window.paint_quad(outline(cursor.bounds, cursor_color, BorderStyle::Solid));
        }
        CursorShape::Block | CursorShape::Bar | CursorShape::Underline => {
            window.paint_quad(fill(cursor.bounds, cursor_color));
        }
    }
    if let Some((origin, glyph)) = &cursor.block_glyph {
        let _ = glyph.paint(*origin, cell_height, TextAlign::Left, None, window, cx);
    }
}

fn cursor_bounds(
    shape: CursorShape,
    cell_origin: Point<Pixels>,
    metrics: CellMetrics,
    span_cols: f32,
) -> Bounds<Pixels> {
    let cell_w = metrics.width * span_cols;
    match shape {
        CursorShape::Block | CursorShape::HollowBlock => {
            Bounds::new(cell_origin, size(cell_w, metrics.height))
        }
        CursorShape::Bar => Bounds::new(cell_origin, size(px(CURSOR_STROKE), metrics.height)),
        CursorShape::Underline => Bounds::new(
            point(
                cell_origin.x,
                cell_origin.y + metrics.height - px(CURSOR_STROKE),
            ),
            size(cell_w, px(CURSOR_STROKE)),
        ),
    }
}

fn shape_cursor_glyph(
    cell: &Cell,
    font_family: &SharedString,
    font_size: Pixels,
    colors: TerminalColors,
    window: &Window,
) -> Option<ShapedLine> {
    if cell.hidden
        || cell.width == Width::Spacer
        || matches!(cell.c, ' ' | '\0' | '\n' | '\r' | '\t')
    {
        return None;
    }
    let mut buf = [0u8; 4];
    let text = cell.c.encode_utf8(&mut buf);
    let run = TextRun {
        len: text.len(),
        font: terminal_font(font_family.clone(), cell.bold, cell.italic),
        color: block_cursor_glyph_color(&colors),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    Some(
        window
            .text_system()
            .shape_line(SharedString::new(text), font_size, &[run], None),
    )
}

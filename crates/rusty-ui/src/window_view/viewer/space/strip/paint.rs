use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{Bounds, Hsla, Window, fill, point, px, size};
use rusty_automotive::space::{Cell, Kind};

use super::cache::{Layout, is_dim};
use crate::tokens::{MemColors, MemKind, Theme};

const CARET_MIN_PX: f32 = 6.;

pub(super) struct Look {
    pub mem: MemColors,
    pub divider: Hsla,
    pub caret: Hsla,
    pub caret_height: f32,
    pub strip_height: f32,
}

impl Look {
    pub fn new(t: &Theme, strip_height: f32) -> Self {
        Self {
            mem: t.mem,
            divider: t.colors.surface,
            caret: t.colors.accent,
            caret_height: f32::from(t.spacing.xxs),
            strip_height,
        }
    }
}

pub(super) struct Scene {
    pub layout: Rc<Layout>,
    pub lit: Option<Rc<[bool]>>,
    pub caret: Option<Range<u32>>,
    pub look: Look,
}

pub(super) fn kind_colors(mem: &MemColors, kind: Kind) -> MemKind {
    match kind {
        Kind::Code => mem.code,
        Kind::Rodata => mem.rodata,
        Kind::Data => mem.data,
        Kind::Bss => mem.bss,
        Kind::Other => mem.other,
        Kind::Fill => mem.fill,
    }
}

struct Pen<'a> {
    bounds: Bounds<gpui_kit::Pixels>,
    factor: f32,
    height: f32,
    window: &'a mut Window,
}

impl Pen<'_> {
    fn quad(&mut self, x: Range<f32>, y: Range<f32>, color: Hsla) {
        let origin = self.bounds.origin;
        let width = (x.end - x.start).max(1.);
        let at = point(origin.x + px(x.start), origin.y + px(y.start));
        let area = Bounds::new(at, size(px(width), px(y.end - y.start)));
        self.window.paint_quad(fill(area, color));
    }

    fn column(&mut self, cell: &Cell, color: Hsla) {
        let x = cell.x0 as f32 * self.factor..cell.x1 as f32 * self.factor;
        let height = self.height;
        self.quad(x, 0.0..height, color);
    }
}

pub(super) fn paint(scene: &Scene, bounds: Bounds<gpui_kit::Pixels>, window: &mut Window) {
    window.paint_layer(bounds, |window| draw(scene, bounds, window));
}

fn draw(scene: &Scene, bounds: Bounds<gpui_kit::Pixels>, window: &mut Window) {
    let Scene {
        layout,
        lit,
        caret,
        look,
    } = scene;
    let width = f32::from(bounds.size.width);
    let height = look.strip_height;
    let mut pen = Pen {
        bounds,
        factor: width / layout.key.width as f32,
        height,
        window,
    };
    pen.quad(0.0..width, 0.0..height, look.mem.free);
    for mark in layout.scale.breaks() {
        let (x0, x1) = (mark.x0 as f32 * width, mark.x1 as f32 * width);
        pen.quad(x0..x0 + 1., 0.0..height, look.mem.free_line);
        pen.quad(x1 - 1.0..x1, 0.0..height, look.mem.free_line);
    }
    let lit = lit.as_deref();
    for cell in &layout.cells {
        let colors = kind_colors(&look.mem, cell.kind);
        let dim = is_dim(lit, &cell.blocks);
        pen.column(cell, if dim { colors.tint } else { colors.strong });
    }
    for inner in &layout.inner {
        let dim = is_dim(lit, &(inner.block..inner.block + 1));
        draw_inner(&mut pen, look, &inner.cells, dim);
    }
    if let Some(span) = caret {
        draw_caret(&mut pen, look, span);
    }
}

fn draw_inner(pen: &mut Pen<'_>, look: &Look, cells: &[Cell], dim: bool) {
    let first = cells.first().map_or(0, |cell| cell.x0);
    for cell in cells {
        let colors = kind_colors(&look.mem, cell.kind);
        pen.column(cell, if dim { colors.tint } else { colors.strong });
        if cell.x0 > first {
            let x = cell.x0 as f32 * pen.factor;
            let height = pen.height;
            pen.quad(x..x + 1., 0.0..height, look.divider);
        }
    }
}

fn draw_caret(pen: &mut Pen<'_>, look: &Look, span: &Range<u32>) {
    let width = f32::from(pen.bounds.size.width);
    let (x0, x1) = (span.start as f32 * pen.factor, span.end as f32 * pen.factor);
    let half = (x1 - x0).max(CARET_MIN_PX) / 2.;
    let middle = ((x0 + x1) / 2.).clamp(half, (width - half).max(half));
    let top = pen.height + look.caret_height;
    pen.quad(
        middle - half..middle + half,
        top..top + look.caret_height,
        look.caret,
    );
}

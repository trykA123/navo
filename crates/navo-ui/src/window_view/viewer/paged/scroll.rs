use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::ScrollbarHandle;
use gpui_kit::{Bounds, Pixels, Point, Size, point, px, size};

const MAX_PAGES: f64 = 10_000.;

#[derive(Clone, Copy, Default)]
struct Geometry {
    bounds: Bounds<Pixels>,
    content: Pixels,
    offset: Pixels,
    dragged: bool,
}

#[derive(Clone, Default)]
pub(super) struct ByteScroll(Rc<Cell<Geometry>>);

impl ByteScroll {
    pub(super) fn height(&self) -> Pixels {
        self.0.get().bounds.size.height
    }

    pub(super) fn measure(&self, bounds: Bounds<Pixels>) -> bool {
        let mut geometry = self.0.get();
        let changed = geometry.bounds.size != bounds.size;
        geometry.bounds = bounds;
        self.0.set(geometry);
        changed
    }

    fn travel(geometry: &Geometry) -> f32 {
        f32::from(geometry.content - geometry.bounds.size.height)
    }

    pub(super) fn take_drag(&self) -> Option<f64> {
        let mut geometry = self.0.get();
        if !geometry.dragged {
            return None;
        }
        geometry.dragged = false;
        self.0.set(geometry);
        let travel = Self::travel(&geometry);
        if travel <= 0. {
            return Some(0.);
        }
        Some(f64::from(-f32::from(geometry.offset) / travel).clamp(0., 1.))
    }

    pub(super) fn show(&self, fraction: f64, pages: f64) {
        let mut geometry = self.0.get();
        let pages = pages.clamp(1., MAX_PAGES);
        geometry.content = geometry.bounds.size.height * pages as f32;
        geometry.offset = px(-(Self::travel(&geometry) * fraction.clamp(0., 1.) as f32));
        self.0.set(geometry);
    }
}

impl ScrollbarHandle for ByteScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.get().bounds
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(0.), self.0.get().offset)
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        let mut geometry = self.0.get();
        geometry.offset = offset.y.min(px(0.)).max(-px(Self::travel(&geometry)));
        geometry.dragged = true;
        self.0.set(geometry);
    }

    fn content_size(&self) -> Size<Pixels> {
        let geometry = self.0.get();
        size(geometry.bounds.size.width, geometry.content)
    }
}

use gpui_kit::{Bounds, Pixels, Point, Size, point, px};

const WIDTH: f32 = 640.;
pub const INPUT_ROW: f32 = 42.;
const MARGIN: f32 = 16.;
const MIN_HEIGHT: f32 = 240.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub origin: Point<Pixels>,
    pub width: Pixels,
    pub max_height: Pixels,
}

pub(crate) fn place(anchor: Bounds<Pixels>, viewport: Size<Pixels>) -> Frame {
    let margin = px(MARGIN);
    let width = px(WIDTH).min(viewport.width - margin * 2.).max(px(0.));
    let right = (viewport.width - margin - width).max(margin);
    let x = (anchor.center().x - width / 2.).max(margin).min(right);
    let y = (anchor.center().y - px(INPUT_ROW / 2.)).max(px(0.));
    let max_height = (viewport.height - y - margin).max(px(MIN_HEIGHT));
    Frame {
        origin: point(x, y),
        width,
        max_height,
    }
}

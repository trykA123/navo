//! The image viewer's pixel grid: 1-px lines between image pixels once the zoom passes 800 %.
//! Drawn on its own canvas over the picture, in the theme's line colour, with no motion.

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Size, Stateful, canvas, fill, point};

use super::ImageView;
use super::zoom::Viewport;
use crate::prelude::*;
use crate::tokens::Theme;

/// Above this scale (image pixels drawn 8 device pixels wide) the grid shows.
pub(super) const GRID_SCALE: f32 = 8.;

/// Where one grid pass runs: the first line, the line spacing and the visible span, in window px.
struct Lines {
    start: f32,
    step: f32,
    from: f32,
    to: f32,
}

impl Lines {
    /// The positions of every line inside the visible span.
    fn positions(&self) -> impl Iterator<Item = f32> + use<> {
        let first = ((self.from - self.start) / self.step).ceil().max(0.) as i64;
        let last = ((self.to - self.start) / self.step).floor() as i64;
        let (start, step) = (self.start, self.step);
        (first..=last).map(move |index| start + index as f32 * step)
    }
}

/// Paints the vertical and horizontal grid lines of an image placed at `image` inside `visible`.
fn paint_grid(
    window: &mut Window,
    image: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    look: (Size<f32>, Hsla),
) {
    let (pixels, color) = look;
    let left = f32::from(image.left());
    let top = f32::from(image.top());
    let columns = Lines {
        start: left,
        step: f32::from(image.size.width) / pixels.width,
        from: f32::from(visible.left()),
        to: f32::from(visible.right()),
    };
    let rows = Lines {
        start: top,
        step: f32::from(image.size.height) / pixels.height,
        from: f32::from(visible.top()),
        to: f32::from(visible.bottom()),
    };
    for x in columns.positions() {
        let line = Bounds::new(
            point(px(x.floor()), visible.top()),
            size(px(1.), visible.size.height),
        );
        window.paint_quad(fill(line, color));
    }
    for y in rows.positions() {
        let line = Bounds::new(
            point(visible.left(), px(y.floor())),
            size(visible.size.width, px(1.)),
        );
        window.paint_quad(fill(line, color));
    }
}

impl ImageView {
    /// True when the current zoom draws image pixels larger than 800 %.
    pub(super) fn shows_grid(&self, viewport: &Viewport) -> bool {
        self.zoom.scale(viewport) > GRID_SCALE
    }

    /// The grid layer over the stage, or None at 800 % and below.
    pub(super) fn grid(&self, t: &Theme) -> Option<ObservedElement<Stateful<Div>>> {
        let viewport = self.viewport();
        if !self.shows_grid(&viewport) {
            return None;
        }
        let (origin, shown) = self.zoom.placement(&viewport);
        let image = Bounds::new(
            point(px(origin.x), px(origin.y)),
            size(px(shown.width), px(shown.height)),
        );
        let look = (self.zoom.image, t.colors.line);
        let layer = canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let visible = bounds.intersect(&image);
                if visible.size.width > px(0.) && visible.size.height > px(0.) {
                    paint_grid(window, image, visible, look);
                }
            },
        )
        .size_full();
        Some(
            div()
                .id("image-grid")
                .test_support()
                .absolute()
                .size_full()
                .child(layer),
        )
    }
}

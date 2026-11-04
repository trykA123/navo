use gpui_kit::{Context, Empty, Pixels, Point, Render, Size, Window, point};

/// The payload of a palette drag; the pointer travel arrives as drag-move events.
#[derive(Clone, Copy)]
pub(crate) struct DraggedPalette;

pub(super) struct PaletteGhost;

impl Render for PaletteGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        Empty
    }
}

/// Keeps a centred panel moved by `offset` inside the window, one axis at a time. A panel that
/// does not fit on an axis is pinned to the margin at the top or left and overflows the rest.
pub(crate) fn clamp_offset(
    offset: Point<Pixels>,
    panel: Size<Pixels>,
    viewport: Size<Pixels>,
    margin: Pixels,
) -> Point<Pixels> {
    point(
        clamp_axis(offset.x, panel.width, viewport.width, margin),
        clamp_axis(offset.y, panel.height, viewport.height, margin),
    )
}

fn clamp_axis(offset: Pixels, panel: Pixels, viewport: Pixels, margin: Pixels) -> Pixels {
    let spare = (viewport - panel) / 2.;
    if panel + margin * 2. > viewport {
        margin - spare
    } else {
        let room = spare - margin;
        offset.max(-room).min(room)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{px, size};

    use super::*;

    const MARGIN: Pixels = px(12.);

    fn clamp(offset: (f32, f32), panel: (f32, f32), view: (f32, f32)) -> (f32, f32) {
        let at = clamp_offset(
            point(px(offset.0), px(offset.1)),
            size(px(panel.0), px(panel.1)),
            size(px(view.0), px(view.1)),
            MARGIN,
        );
        (f32::from(at.x), f32::from(at.y))
    }

    #[test]
    fn zero_and_inside_offsets_stay() {
        assert_eq!(clamp((0., 0.), (400., 300.), (800., 600.)), (0., 0.));
        assert_eq!(clamp((50., -40.), (400., 300.), (800., 600.)), (50., -40.));
    }

    #[test]
    fn the_four_edges_stop_at_the_margin() {
        let panel = (400., 300.);
        let view = (800., 600.);
        assert_eq!(clamp((999., 0.), panel, view).0, 188.);
        assert_eq!(clamp((-999., 0.), panel, view).0, -188.);
        assert_eq!(clamp((0., 999.), panel, view).1, 138.);
        assert_eq!(clamp((0., -999.), panel, view).1, -138.);
    }

    #[test]
    fn a_panel_wider_than_the_window_is_pinned_left_and_height_stays_clamped() {
        let at = clamp((30., 999.), (700., 300.), (390., 600.));
        assert_eq!(at, (12. + 155., 138.));
        let origin = (390. - 700.) / 2. + at.0;
        assert_eq!(origin, 12.);
    }

    #[test]
    fn a_panel_taller_than_the_window_is_pinned_top() {
        let at = clamp((0., -50.), (400., 500.), (800., 400.));
        assert_eq!(at.1, 12. + 50.);
        assert_eq!((400. - 500.) / 2. + at.1, 12.);
        assert_eq!(at.0, 0.);
    }

    #[test]
    fn a_window_that_shrank_pulls_the_panel_back() {
        assert_eq!(clamp((188., 0.), (400., 300.), (600., 600.)).0, 88.);
        assert_eq!(clamp((188., 0.), (400., 300.), (460., 600.)).0, 18.);
    }
}

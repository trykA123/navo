use gpui_kit::{BoxShadow, Hsla, point, px};

use super::design_b::ok;
use super::{Mode, Surfaces};

#[derive(Clone, PartialEq, Debug)]
pub struct Shadows {
    pub card: Vec<BoxShadow>,
    pub card_active: Vec<BoxShadow>,
    pub job_card: Vec<BoxShadow>,
    pub pill: Vec<BoxShadow>,
    pub keycap: Vec<BoxShadow>,
    pub menu: Vec<BoxShadow>,
    pub chip: Vec<BoxShadow>,
    pub sheet: Vec<BoxShadow>,
    pub sheet_ring: Vec<BoxShadow>,
    pub focus: Vec<BoxShadow>,
    pub up: Vec<BoxShadow>,
}

#[derive(Clone, Copy)]
struct Layer {
    color: Hsla,
    y: f32,
    blur: f32,
    spread: f32,
}

fn drop(color: Hsla, y: f32, blur: f32, spread: f32) -> Layer {
    Layer {
        color,
        y,
        blur,
        spread,
    }
}

fn ring(color: Hsla, width: f32) -> Layer {
    drop(color, 0., 0., width)
}

fn shadow(layers: impl IntoIterator<Item = Layer>) -> Vec<BoxShadow> {
    layers
        .into_iter()
        .map(|layer| BoxShadow {
            color: layer.color,
            offset: point(px(0.), px(layer.y)),
            // GPUI blurs by the Gaussian sigma (CSS blur is twice that) and paints later layers on top.
            blur_radius: px(layer.blur / 2.),
            spread_radius: px(layer.spread),
            inset: false,
        })
        .collect()
}

struct Depth {
    card: [Layer; 2],
    pop: Layer,
    menu: [Layer; 2],
    chip: [Layer; 2],
    sheet: Layer,
    up: Layer,
}

fn depth(mode: Mode) -> Depth {
    match mode {
        Mode::Light => {
            let (ink, ui) = (ok(20., 0.02, 250.), ok(21., 0.012, 260.));
            Depth {
                card: [
                    drop(ink.opacity(0.08), 1., 2., 0.),
                    drop(ink.opacity(0.22), 8., 24., -12.),
                ],
                pop: drop(ink.opacity(0.35), 24., 48., -16.),
                menu: [
                    drop(ui.opacity(0.28), 18., 44., -12.),
                    drop(ui.opacity(0.06), 2., 6., 0.),
                ],
                chip: [
                    drop(ui.opacity(0.08), 2., 6., 0.),
                    drop(ui.opacity(0.2), 8., 20., -10.),
                ],
                sheet: drop(ink.opacity(0.42), 28., 60., -20.),
                up: drop(ink.opacity(0.35), -18., 40., -18.),
            }
        }
        Mode::Dark => {
            let black = ok(0., 0., 0.);
            Depth {
                card: [
                    drop(black.opacity(0.4), 1., 2., 0.),
                    drop(black.opacity(0.6), 8., 24., -12.),
                ],
                pop: drop(black.opacity(0.7), 24., 48., -16.),
                menu: [
                    drop(black.opacity(0.7), 22., 48., -12.),
                    ring(ok(93., 0.006, 260.).opacity(0.04), 1.),
                ],
                chip: [
                    drop(black.opacity(0.35), 2., 6., 0.),
                    drop(black.opacity(0.6), 10., 24., -10.),
                ],
                sheet: drop(black.opacity(0.85), 28., 60., -20.),
                up: drop(black.opacity(0.75), -18., 40., -18.),
            }
        }
    }
}

pub(super) fn shadows(mode: Mode, s: &Surfaces) -> Shadows {
    let d = depth(mode);
    let sheet = [d.sheet, ring(s.card_edge, 1.)];
    Shadows {
        card: shadow([ring(s.card_edge, 1.)]),
        card_active: shadow(d.card.into_iter().chain([ring(s.card_ring, 1.)])),
        job_card: shadow([d.pop, ring(s.card_edge, 1.)]),
        pill: shadow([ring(s.pill_ring, 1.5)]),
        keycap: shadow([drop(s.keycap_edge, 1., 0., 0.)]),
        menu: shadow(d.menu),
        chip: shadow(d.chip),
        sheet: shadow(sheet),
        sheet_ring: shadow(sheet.into_iter().chain([ring(s.pill_ring, 1.5)])),
        focus: shadow([ring(s.pill_ring, 2.)]),
        up: shadow([d.up, ring(s.card_edge, 1.)]),
    }
}

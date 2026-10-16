use gpui_kit::{BoxShadow, Hsla, point, px};

use super::Mode;
use super::design_b::ok;

#[derive(Clone, PartialEq, Debug)]
pub struct Shadows {
    pub card: Vec<BoxShadow>,
    pub job_card: Vec<BoxShadow>,
}

struct Layer {
    color: Hsla,
    y: f32,
    blur: f32,
    spread: f32,
}

fn layer(color: Hsla, y: f32, blur: f32, spread: f32) -> Layer {
    Layer {
        color,
        y,
        blur,
        spread,
    }
}

fn shadow(layers: impl IntoIterator<Item = Layer>) -> Vec<BoxShadow> {
    layers
        .into_iter()
        .map(|layer| BoxShadow {
            color: layer.color,
            offset: point(px(0.), px(layer.y)),
            // GPUI blurs by the Gaussian sigma; a CSS blur radius is twice that.
            blur_radius: px(layer.blur / 2.),
            spread_radius: px(layer.spread),
            inset: false,
        })
        .collect()
}

pub(super) fn shadows(mode: Mode) -> Shadows {
    match mode {
        Mode::Light => {
            let ink = ok(20., 0.03, 255.);
            Shadows {
                card: shadow([
                    layer(ink.opacity(0.06), 1., 2., 0.),
                    layer(ink.opacity(0.3), 10., 30., -18.),
                ]),
                job_card: shadow([
                    layer(ink.opacity(0.08), 0., 0., 1.),
                    layer(ink.opacity(0.32), 16., 40., -12.),
                ]),
            }
        }
        Mode::Dark => {
            let (white, black) = (ok(100., 0., 0.), ok(0., 0., 0.));
            Shadows {
                card: shadow([
                    layer(white.opacity(0.04), 0., 0., 1.),
                    layer(black.opacity(0.8), 14., 34., -16.),
                ]),
                job_card: shadow([
                    layer(white.opacity(0.07), 0., 0., 1.),
                    layer(black.opacity(0.75), 18., 44., -12.),
                ]),
            }
        }
    }
}

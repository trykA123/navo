use gpui_kit::Hsla;
use gpui_kit::component::theme::oklch;

use super::Mode;

const HUES: [f32; 8] = [25., 55., 95., 145., 190., 240., 300., 350.];

const LIGHT_TICK_CHROMA: [f32; 8] = [0.2, 0.138, 0.114, 0.176, 0.096, 0.128, 0.2, 0.2];
const DARK_FILL_CHROMA: [f32; 8] = [0.124, 0.094, 0.078, 0.118, 0.064, 0.086, 0.124, 0.124];
const DARK_TICK_CHROMA: [f32; 8] = [0.15, 0.15, 0.146, 0.15, 0.124, 0.15, 0.15, 0.15];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MarkColors {
    pub fills: [Hsla; 8],
    pub ticks: [Hsla; 8],
}

fn scale(lightness: f32, chroma: [f32; 8]) -> [Hsla; 8] {
    std::array::from_fn(|at| oklch(lightness, chroma[at], HUES[at]))
}

pub(super) fn mark_colors(mode: Mode) -> MarkColors {
    match mode {
        Mode::Light => MarkColors {
            fills: scale(0.89, [0.056; 8]),
            ticks: scale(0.56, LIGHT_TICK_CHROMA),
        },
        Mode::Dark => MarkColors {
            fills: scale(0.38, DARK_FILL_CHROMA),
            ticks: scale(0.72, DARK_TICK_CHROMA),
        },
    }
}

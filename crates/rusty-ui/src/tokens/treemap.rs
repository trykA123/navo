use gpui_kit::Hsla;
use gpui_kit::component::theme::oklch;

use super::Mode;

const CHROMA_STEP: f32 = 0.01;

#[derive(Clone, Copy, Debug)]
pub struct TreemapLook {
    pub lightness: f32,
    pub chroma: f32,
    pub amplitude: f32,
    ink: (f32, f32),
}

impl TreemapLook {
    pub fn fill(self, level: f32, hue: f32) -> Hsla {
        oklch(
            self.lightness + self.amplitude * level,
            self.chroma + CHROMA_STEP * level,
            hue,
        )
    }

    pub fn ink(self, hue: f32) -> Hsla {
        oklch(self.ink.0, self.ink.1, hue)
    }
}

pub fn treemap_look(mode: Mode) -> TreemapLook {
    match mode {
        Mode::Light => TreemapLook {
            lightness: 0.89,
            chroma: 0.05,
            amplitude: 0.035,
            ink: (0.3, 0.03),
        },
        Mode::Dark => TreemapLook {
            lightness: 0.36,
            chroma: 0.055,
            amplitude: 0.045,
            ink: (0.93, 0.02),
        },
    }
}

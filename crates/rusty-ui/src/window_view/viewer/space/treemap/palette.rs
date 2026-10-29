use gpui_kit::Hsla;
use gpui_kit::component::theme::oklch;

use crate::tokens::Mode;

const START_HUE: f32 = 40.;
const GOLDEN_ANGLE: f32 = 137.508;
const LEVELS: [f32; 4] = [1., -1., 0.5, -0.5];
const CHROMA_STEP: f32 = 0.01;

#[derive(Clone, Copy, Debug)]
struct Look {
    lightness: f32,
    chroma: f32,
    amplitude: f32,
    ink: (f32, f32),
}

fn look(mode: Mode) -> Look {
    match mode {
        Mode::Light => Look {
            lightness: 0.89,
            chroma: 0.05,
            amplitude: 0.035,
            ink: (0.3, 0.03),
        },
        Mode::Dark => Look {
            lightness: 0.36,
            chroma: 0.055,
            amplitude: 0.045,
            ink: (0.93, 0.02),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    pub bg: Hsla,
    pub ink: Hsla,
}

#[derive(Clone, Copy, Debug)]
pub struct GroupColours {
    pub hue: f32,
    mode: Mode,
}

impl GroupColours {
    pub fn base(self) -> Shade {
        self.shade(0.)
    }

    pub fn part(self, rank: usize) -> Shade {
        self.shade(LEVELS[rank % LEVELS.len()])
    }

    #[cfg(test)]
    pub fn lightness(self, rank: usize) -> f32 {
        let look = look(self.mode);
        look.lightness + look.amplitude * LEVELS[rank % LEVELS.len()]
    }

    fn shade(self, level: f32) -> Shade {
        let look = look(self.mode);
        Shade {
            bg: oklch(
                look.lightness + look.amplitude * level,
                look.chroma + CHROMA_STEP * level,
                self.hue,
            ),
            ink: oklch(look.ink.0, look.ink.1, self.hue),
        }
    }
}

pub fn palette(count: usize, mode: Mode) -> Vec<GroupColours> {
    (0..count)
        .map(|at| GroupColours {
            hue: (START_HUE + GOLDEN_ANGLE * at as f32).rem_euclid(360.),
            mode,
        })
        .collect()
}

#[cfg(test)]
mod tests;

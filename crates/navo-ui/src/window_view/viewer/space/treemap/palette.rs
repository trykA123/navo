use gpui_kit::Hsla;

use crate::tokens::Mode;
use crate::tokens::treemap_look as look;

const START_HUE: f32 = 40.;
const GOLDEN_ANGLE: f32 = 137.508;
const LEVELS: [f32; 4] = [1., -1., 0.5, -0.5];

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
            bg: look.fill(level, self.hue),
            ink: look.ink(self.hue),
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

use gpui_kit::Hsla;

use super::Mode;
use super::palette::{Tokens, lch};

/// Colours for find hits: the band behind a matching line, its text and a failed search.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FindColors {
    /// The band behind a matching line; 3:1 or more against the raised surface.
    pub hit: Hsla,
    /// The text on a matching line; 4.5:1 or more against the band.
    pub hit_text: Hsla,
    /// A failed search's notice.
    pub bad: Hsla,
}

/// Band lightness that holds 3:1 against the raised surface while the text keeps 4.5:1.
const HIT_LIGHTNESS_LIGHT: f32 = 66.;
const HIT_LIGHTNESS_DARK: f32 = 56.;
/// Warm chroma for the band, enough to read as a find hit rather than a hover.
const HIT_CHROMA: f32 = 0.13;

/// Dark bands sit on a dark surface, so their text goes brighter than `text`.
const HIT_TEXT_DARK_LIGHTNESS: f32 = 100.;

pub(super) fn find_colors(t: &Tokens, mode: Mode) -> FindColors {
    let (lightness, hit_text) = match mode {
        Mode::Light => (HIT_LIGHTNESS_LIGHT, t.text.color()),
        Mode::Dark => (
            HIT_LIGHTNESS_DARK,
            lch(HIT_TEXT_DARK_LIGHTNESS, 0., 0.).color(),
        ),
    };
    FindColors {
        hit: lch(lightness, HIT_CHROMA, t.warning.h).color(),
        hit_text,
        bad: t.danger.color(),
    }
}

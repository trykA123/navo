use gpui_kit::Hsla;

use super::Mode;
use super::palette::Tokens;

/// Colours for find hits: the band behind a matching line, its text and a failed search.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FindColors {
    pub hit: Hsla,
    pub hit_text: Hsla,
    pub bad: Hsla,
}

/// How far the raised surface moves toward the warning hue to make a hit visible.
const HIT_TOWARD_WARNING_LIGHT: f32 = 0.20;
const HIT_TOWARD_WARNING_DARK: f32 = 0.25;

pub(super) fn find_colors(t: &Tokens, mode: Mode) -> FindColors {
    let amount = match mode {
        Mode::Light => HIT_TOWARD_WARNING_LIGHT,
        Mode::Dark => HIT_TOWARD_WARNING_DARK,
    };
    FindColors {
        hit: t.raised.mix(t.warning, amount).color(),
        hit_text: t.text.color(),
        bad: t.danger.color(),
    }
}

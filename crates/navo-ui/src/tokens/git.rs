use super::Mode;
use super::palette::{Lch, Tokens, lch};
use gpui_kit::Hsla;

/// Git state colours for the file list gutter. The clean tick uses `Colors::ok`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GitColors {
    pub conflict: Hsla,
    pub modified: Hsla,
    pub sync: Hsla,
    pub quiet: Hsla,
}

impl GitColors {
    pub(super) fn uniform(color: Hsla) -> Self {
        Self {
            conflict: color,
            modified: color,
            sync: color,
            quiet: color,
        }
    }
}

/// Hue gaps from the danger, warning and info tokens to the glyph hues.
const CONFLICT_HUE_SHIFT: f32 = 3.;
const MODIFIED_HUE_SHIFT: f32 = -25.;
const SYNC_HUE_SHIFT: f32 = -22.;
/// Moves the quiet glyph off the faint line colour until it holds 3:1 on the selected row.
const QUIET_TOWARD_MUTED: f32 = 0.5;

pub(super) fn git_colors(t: &Tokens, mode: Mode) -> GitColors {
    let (lightness, conflict_chroma, modified_chroma, sync_chroma) = match mode {
        Mode::Light => (56., 0.2, 0.138, 0.128),
        Mode::Dark => (72., 0.15, 0.15, 0.15),
    };
    let quiet = t.border_strong.mix(t.muted, QUIET_TOWARD_MUTED);
    let glyph = |chroma: f32, base: Lch, shift: f32| lch(lightness, chroma, base.h + shift).color();
    GitColors {
        conflict: glyph(conflict_chroma, t.danger, CONFLICT_HUE_SHIFT),
        modified: glyph(modified_chroma, t.warning, MODIFIED_HUE_SHIFT),
        sync: glyph(sync_chroma, t.info, SYNC_HUE_SHIFT),
        quiet: quiet.color(),
    }
}

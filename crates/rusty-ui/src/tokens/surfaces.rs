use gpui_kit::{Background, ColorSpace, Hsla, linear_color_stop, linear_gradient};

use super::palette::{Tokens, lch};
use super::{Colors, Mode};

const MICA_ANGLE: f32 = 135.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Surfaces {
    pub mica_from: Hsla,
    pub mica_to: Hsla,
    pub card: Hsla,
    pub card_edge: Hsla,
    pub card_ring: Hsla,
    pub current: Hsla,
    pub field: Hsla,
    pub keycap_face: Hsla,
    pub keycap_edge: Hsla,
    pub keycap_label: Hsla,
    pub keycap_lit: Hsla,
    pub keycap_lit_label: Hsla,
    pub layer_tint: Hsla,
    pub pill: Hsla,
    pub pill_ring: Hsla,
    pub pill_label: Hsla,
    pub progress_track: Hsla,
    pub progress_fill: Hsla,
    pub job_card: Hsla,
    pub sheet: Hsla,
    pub scrim: Hsla,
    pub tile: Hsla,
    pub tile_edge: Hsla,
}

impl Surfaces {
    pub fn mica(&self) -> Background {
        linear_gradient(
            MICA_ANGLE,
            linear_color_stop(self.mica_from, 0.),
            linear_color_stop(self.mica_to, 1.),
        )
        .color_space(ColorSpace::Oklab)
    }

    pub fn entries(&self) -> [(&'static str, Hsla); 23] {
        let Self {
            mica_from,
            mica_to,
            card,
            card_edge,
            card_ring,
            current,
            field,
            keycap_face,
            keycap_edge,
            keycap_label,
            keycap_lit,
            keycap_lit_label,
            layer_tint,
            pill,
            pill_ring,
            pill_label,
            progress_track,
            progress_fill,
            job_card,
            sheet,
            scrim,
            tile,
            tile_edge,
        } = *self;
        [
            ("mica_from", mica_from),
            ("mica_to", mica_to),
            ("card", card),
            ("card_edge", card_edge),
            ("card_ring", card_ring),
            ("current", current),
            ("field", field),
            ("keycap_face", keycap_face),
            ("keycap_edge", keycap_edge),
            ("keycap_label", keycap_label),
            ("keycap_lit", keycap_lit),
            ("keycap_lit_label", keycap_lit_label),
            ("layer_tint", layer_tint),
            ("pill", pill),
            ("pill_ring", pill_ring),
            ("pill_label", pill_label),
            ("progress_track", progress_track),
            ("progress_fill", progress_fill),
            ("job_card", job_card),
            ("sheet", sheet),
            ("scrim", scrim),
            ("tile", tile),
            ("tile_edge", tile_edge),
        ]
    }
}

fn build(c: &Colors, t: &Tokens, scrim: Hsla) -> Surfaces {
    let (surface, line, accent) = (c.surface, c.line, c.accent);
    Surfaces {
        mica_from: c.rail,
        mica_to: t.bg.mix(t.selected, 0.3).color(),
        card: surface,
        card_edge: line,
        card_ring: t.accent.mix(t.surface, 0.2).color(),
        current: c.selection,
        field: c.rail,
        keycap_face: surface,
        keycap_edge: line,
        keycap_label: c.text,
        keycap_lit: accent,
        keycap_lit_label: c.background,
        layer_tint: c.hover,
        pill: surface,
        pill_ring: accent,
        pill_label: c.accent_text,
        progress_track: line,
        progress_fill: accent,
        job_card: c.background,
        sheet: surface,
        scrim,
        tile: surface,
        tile_edge: line,
    }
}

pub(super) fn surfaces(mode: Mode, colors: &Colors, tokens: &Tokens) -> Surfaces {
    match mode {
        Mode::Light => build(colors, tokens, colors.text.opacity(0.16)),
        Mode::Dark => build(
            colors,
            tokens,
            lch(5., tokens.bg.c / 2., tokens.bg.h).color().opacity(0.42),
        ),
    }
}

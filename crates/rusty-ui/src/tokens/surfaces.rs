use gpui_kit::{Background, ColorSpace, Hsla, linear_color_stop, linear_gradient};

use super::design_b::ok;
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

fn light(c: &Colors) -> Surfaces {
    let (surface, line, accent) = (ok(99.3, 0.002, 250.), ok(88., 0.01, 250.), c.accent);
    Surfaces {
        mica_from: ok(93.8, 0.012, 245.),
        mica_to: ok(94.5, 0.014, 60.),
        card: surface,
        card_edge: line,
        card_ring: accent,
        current: ok(91.5, 0.045, 262.),
        field: ok(96.5, 0.006, 250.),
        keycap_face: ok(99.8, 0.001, 260.),
        keycap_edge: ok(84., 0.01, 260.),
        keycap_label: c.text,
        keycap_lit: c.accent,
        keycap_lit_label: c.background,
        layer_tint: ok(94.5, 0.03, 262.),
        pill: surface,
        pill_ring: accent,
        pill_label: c.accent_text,
        progress_track: line,
        progress_fill: accent,
        job_card: ok(98.6, 0.004, 250.),
        sheet: ok(99.8, 0.001, 260.),
        scrim: ok(21., 0.012, 260.).opacity(0.16),
        tile: ok(99.8, 0.001, 260.),
        tile_edge: ok(92.9, 0.002, 260.),
    }
}

fn dark(c: &Colors) -> Surfaces {
    let (surface, line, accent) = (ok(20.5, 0.01, 250.), ok(29., 0.012, 250.), c.accent);
    Surfaces {
        mica_from: ok(15.5, 0.014, 250.),
        mica_to: ok(17., 0.014, 280.),
        card: surface,
        card_edge: line,
        card_ring: accent,
        current: ok(32., 0.07, 262.),
        field: ok(17.5, 0.01, 250.),
        keycap_face: ok(24., 0.012, 260.),
        keycap_edge: ok(11., 0.008, 260.),
        keycap_label: c.text,
        keycap_lit: c.accent,
        keycap_lit_label: c.background,
        layer_tint: ok(23., 0.05, 262.),
        pill: surface,
        pill_ring: accent,
        pill_label: c.accent_text,
        progress_track: line,
        progress_fill: accent,
        job_card: ok(23., 0.012, 250.),
        sheet: ok(20.5, 0.011, 260.),
        scrim: ok(5., 0.005, 260.).opacity(0.42),
        tile: ok(22.6, 0.012, 260.),
        tile_edge: ok(31., 0.01, 260.),
    }
}

pub(super) fn surfaces(mode: Mode, colors: &Colors) -> Surfaces {
    match mode {
        Mode::Light => light(colors),
        Mode::Dark => dark(colors),
    }
}

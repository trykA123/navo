use gpui_kit::{Background, ColorSpace, Hsla, linear_color_stop, linear_gradient};

use super::design_b::ok;
use super::{Colors, Mode};

const MICA_ANGLE: f32 = 160.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Surfaces {
    pub mica_from: Hsla,
    pub mica_to: Hsla,
    pub card: Hsla,
    pub card_ring: Hsla,
    pub field: Hsla,
    pub keycap_face: Hsla,
    pub keycap_edge: Hsla,
    pub keycap_label: Hsla,
    pub keycap_lit: Hsla,
    pub keycap_lit_label: Hsla,
    pub pill: Hsla,
    pub pill_ring: Hsla,
    pub progress_track: Hsla,
    pub progress_fill: Hsla,
    pub job_card: Hsla,
    pub sheet: Hsla,
    pub scrim: Hsla,
    pub tile: Hsla,
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

    pub fn entries(&self) -> [(&'static str, Hsla); 18] {
        let Self {
            mica_from,
            mica_to,
            card,
            card_ring,
            field,
            keycap_face,
            keycap_edge,
            keycap_label,
            keycap_lit,
            keycap_lit_label,
            pill,
            pill_ring,
            progress_track,
            progress_fill,
            job_card,
            sheet,
            scrim,
            tile,
        } = *self;
        [
            ("mica_from", mica_from),
            ("mica_to", mica_to),
            ("card", card),
            ("card_ring", card_ring),
            ("field", field),
            ("keycap_face", keycap_face),
            ("keycap_edge", keycap_edge),
            ("keycap_label", keycap_label),
            ("keycap_lit", keycap_lit),
            ("keycap_lit_label", keycap_lit_label),
            ("pill", pill),
            ("pill_ring", pill_ring),
            ("progress_track", progress_track),
            ("progress_fill", progress_fill),
            ("job_card", job_card),
            ("sheet", sheet),
            ("scrim", scrim),
            ("tile", tile),
        ]
    }
}

fn light(c: &Colors) -> Surfaces {
    Surfaces {
        mica_from: ok(95.4, 0.01, 255.),
        mica_to: ok(92.6, 0.016, 265.),
        card: ok(99.4, 0.002, 260.),
        card_ring: c.accent,
        field: ok(96.4, 0.006, 260.),
        keycap_face: ok(99.4, 0.002, 260.),
        keycap_edge: ok(82., 0.012, 260.),
        keycap_label: c.text,
        keycap_lit: c.accent,
        keycap_lit_label: c.background,
        pill: ok(99.4, 0.002, 260.),
        pill_ring: ok(86., 0.012, 260.),
        progress_track: ok(90.5, 0.01, 260.),
        progress_fill: c.accent,
        job_card: ok(99.6, 0.002, 260.),
        sheet: ok(99.6, 0.002, 260.),
        scrim: ok(20., 0.02, 260.).opacity(0.28),
        tile: ok(95.6, 0.008, 260.),
    }
}

fn dark(c: &Colors) -> Surfaces {
    Surfaces {
        mica_from: ok(14.5, 0.012, 262.),
        mica_to: ok(11.5, 0.01, 255.),
        card: ok(19.5, 0.01, 260.),
        card_ring: c.accent,
        field: ok(23.5, 0.012, 260.),
        keycap_face: ok(25., 0.012, 260.),
        keycap_edge: ok(12., 0.008, 260.),
        keycap_label: c.text,
        keycap_lit: c.accent,
        keycap_lit_label: c.background,
        pill: ok(22.5, 0.011, 260.),
        pill_ring: ok(34., 0.014, 260.),
        progress_track: ok(30., 0.014, 260.),
        progress_fill: c.accent,
        job_card: ok(22.5, 0.011, 260.),
        sheet: ok(21., 0.011, 260.),
        scrim: ok(0., 0., 0.).opacity(0.55),
        tile: ok(25., 0.012, 260.),
    }
}

pub(super) fn surfaces(mode: Mode, colors: &Colors) -> Surfaces {
    match mode {
        Mode::Light => light(colors),
        Mode::Dark => dark(colors),
    }
}

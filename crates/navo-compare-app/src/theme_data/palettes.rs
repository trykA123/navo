//! The five Sidewise palettes with their dimensions and motion timings (NC66, NC68).

use std::sync::LazyLock;

use gpui_kit::{Hsla, Pixels, px};
use navo_compare_profile::appearance::PaletteId;

/// OKLCH as the prototype writes it: lightness in percent, chroma, hue in degrees.
pub(crate) type Oklch = (f32, f32, f32);

/// The W3C CSS Color 4 OKLCH to linear-sRGB matrix, before any gamut mapping.
#[expect(
    clippy::excessive_precision,
    reason = "the published matrix constants, kept verbatim; f32 rounds them identically"
)]
fn linear((l, c, h): Oklch) -> [f32; 3] {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    ]
}

/// Converts a token to `Hsla`, giving up chroma rather than hue or lightness
/// until it fits sRGB, so a clipped colour keeps its family instead of shifting.
pub(crate) fn oklch((l, c, h): Oklch) -> Hsla {
    let l = l / 100.0;
    let fits = |c: f32| linear((l, c, h)).iter().all(|v| (0.0..=1.0).contains(v));
    let mut chroma = c;
    if !fits(c) {
        chroma = 0.0;
        let mut step = c / 2.0;
        for _ in 0..24 {
            if fits(chroma + step) {
                chroma += step;
            }
            step /= 2.0;
        }
    }
    gpui_kit::component::theme::oklch(l, chroma, h)
}

/// One mode's tokens, in the order of the ramp arrays below.
#[derive(Clone, Copy, Debug)]
#[rustfmt::skip]
pub struct ModeTokens {
    pub surface: Hsla, pub surface2: Hsla, pub card: Hsla, pub layer2: Hsla, pub layer3: Hsla,
    pub text: Hsla, pub muted: Hsla, pub faint: Hsla, pub accent: Hsla, pub on_accent: Hsla,
    pub selected: Hsla, pub accent_tint: Hsla, pub sep: Hsla, pub sep_strong: Hsla,
    pub toast: [Hsla; 3], pub art: [Hsla; 4], pub shadow: [Hsla; 3], pub scrim: Hsla,
}

/// A palette in both modes, with the focus ring width of NC66.
#[derive(Clone, Copy, Debug)]
#[rustfmt::skip]
pub struct PaletteSpec { pub light: ModeTokens, pub dark: ModeTokens, pub ring_px: f32 }

/// Shadow and scrim alphas of the prototype, shared by every palette: light, then dark.
const SHADOW_A: [[f32; 3]; 2] = [[0.05, 0.18, 0.32], [0.5, 0.8, 0.88]];
const SCRIM_A: [f32; 2] = [0.34, 0.62];

/// Maps a ramp onto the named tokens, applying the shared shadow and scrim alphas.
#[rustfmt::skip]
fn mode(v: [Oklch; 23], m: usize) -> ModeTokens {
    let [surface, surface2, card, layer2, layer3, text, muted, faint, accent, on_accent, selected,
         accent_tint, sep, sep_strong, toast_bg, toast_fg, toast_acc, a1, a2, b1, b2, shadow, scrim] = v.map(oklch);
    let shadow = SHADOW_A[m].map(|a| Hsla { a, ..shadow });
    ModeTokens { surface, surface2, card, layer2, layer3, text, muted, faint, accent, on_accent,
        selected, accent_tint, sep, sep_strong, toast: [toast_bg, toast_fg, toast_acc],
        art: [a1, a2, b1, b2], shadow, scrim: Hsla { a: SCRIM_A[m], ..scrim } }
}

/// Converts one palette's light and dark ramps into a spec.
#[rustfmt::skip]
fn spec([light, dark]: [[Oklch; 23]; 2], ring_px: f32) -> PaletteSpec {
    PaletteSpec { light: mode(light, 0), dark: mode(dark, 1), ring_px }
}

/// Every dimension the prototype tokenizes (NC66), in pixels.
#[derive(Clone, Copy, Debug)]
#[rustfmt::skip]
pub struct DimTokens {
    pub card_r: Pixels, pub row_r: Pixels, pub btn_r: Pixels, pub sheet_r: Pixels,
    pub row_h: Pixels, pub gut_w: Pixels, pub map_w: Pixels, pub ln_w: Pixels, pub fs: Pixels,
    pub code_fs: Pixels, pub code_lh: Pixels, pub inset_x: Pixels, pub inset_y: Pixels,
    pub sidebar_w: Pixels, pub rail_w: Pixels, pub start_field_min_w: Pixels,
}

/// The `:root` dimensions of the prototype.
#[rustfmt::skip]
pub const DIMS: DimTokens = DimTokens {
    card_r: px(16.), row_r: px(12.), btn_r: px(10.), sheet_r: px(22.), row_h: px(30.),
    gut_w: px(98.), map_w: px(14.), ln_w: px(52.), fs: px(14.), code_fs: px(13.),
    code_lh: px(21.), inset_x: px(16.), inset_y: px(12.), sidebar_w: px(280.), rail_w: px(56.),
    start_field_min_w: px(224.),
};

/// Transition durations in milliseconds (NC68); the UI snaps them to 0 on reduced motion.
#[derive(Clone, Copy, Debug)]
#[rustfmt::skip]
pub struct MotionTokens {
    pub hover_ms: u64, pub menu_ms: u64, pub theme_ms: u64, pub screen_ms: u64,
    pub row_ms: u64, pub side_ms: u64, pub sheet_ms: u64,
}

/// The `--dur-*` timings of the prototype.
#[rustfmt::skip]
pub const MOTION: MotionTokens = MotionTokens {
    hover_ms: 150, menu_ms: 180, theme_ms: 200, screen_ms: 220, row_ms: 220, side_ms: 240,
    sheet_ms: 240,
};

/// The five palettes of NC66, keyed by the persisted `PaletteId`.
#[rustfmt::skip]
pub static PALETTES: LazyLock<[(PaletteId, PaletteSpec); 5]> = LazyLock::new(|| [
    (PaletteId::Violet, spec(VIOLET, 2.0)), (PaletteId::Graphite, spec(GRAPHITE, 2.0)),
    (PaletteId::Ocean, spec(OCEAN, 2.0)), (PaletteId::Ember, spec(EMBER, 2.0)),
    (PaletteId::Contrast, spec(CONTRAST, 3.0)),
]);

/// The prototype's OKLCH tokens, light then dark, in `ModeTokens` order: surface,
/// surface2, card, layer2, layer3, text, muted, faint, accent, on_accent, selected,
/// accent_tint, sep, sep_strong, toast bg/fg/accent, art a1/a2/b1/b2, shadow, scrim.
#[rustfmt::skip]
const VIOLET: [[Oklch; 23]; 2] = [
    [(97.,0.006,285.), (94.5,0.008,285.), (100.,0.,285.), (95.,0.008,285.), (91.5,0.011,285.),
     (21.,0.013,285.), (49.,0.02,285.), (53.,0.018,285.), (53.,0.17,285.), (98.,0.008,285.),
     (93.,0.033,285.), (95.,0.023,285.), (90.,0.008,285.), (60.5,0.017,285.), (21.,0.013,285.),
     (97.,0.006,285.), (64.,0.17,285.), (55.,0.16,285.), (38.,0.12,325.), (55.,0.16,265.),
     (38.,0.12,305.), (25.,0.03,285.), (20.,0.02,285.)],
    [(13.,0.013,285.), (17.,0.014,285.), (19.5,0.014,285.), (25.,0.016,285.), (31.,0.018,285.),
     (96.,0.007,285.), (76.,0.02,285.), (70.,0.018,285.), (69.5,0.166,285.), (17.,0.05,285.),
     (32.,0.085,285.), (26.,0.048,285.), (30.,0.014,285.), (64.,0.017,285.), (93.,0.01,285.),
     (18.,0.014,285.), (53.,0.17,285.), (52.,0.16,285.), (34.,0.12,325.), (52.,0.16,265.),
     (34.,0.12,305.), (0.,0.,0.), (0.,0.,0.)],
];

#[rustfmt::skip]
const GRAPHITE: [[Oklch; 23]; 2] = [
    [(97.,0.002,265.), (94.5,0.002,265.), (100.,0.,265.), (95.,0.002,265.), (91.5,0.003,265.),
     (21.,0.004,265.), (49.,0.006,265.), (52.5,0.005,265.), (52.,0.15,255.), (98.,0.008,255.),
     (93.,0.033,255.), (95.,0.023,255.), (90.,0.002,265.), (60.5,0.005,265.), (21.,0.004,265.),
     (97.,0.002,265.), (64.,0.15,255.), (55.,0.16,255.), (38.,0.12,295.), (55.,0.116,235.),
     (38.,0.12,275.), (25.,0.03,265.), (20.,0.02,265.)],
    [(13.,0.004,265.), (17.,0.004,265.), (19.5,0.004,265.), (25.,0.005,265.), (31.,0.005,265.),
     (96.,0.002,265.), (76.,0.006,265.), (70.,0.005,265.), (69.,0.15,255.), (17.,0.05,255.),
     (32.,0.075,255.), (26.,0.042,255.), (30.,0.004,265.), (64.,0.005,265.), (93.,0.003,265.),
     (18.,0.004,265.), (52.,0.15,255.), (52.,0.16,255.), (34.,0.12,295.), (52.,0.11,235.),
     (34.,0.12,275.), (0.,0.,0.), (0.,0.,0.)],
];

#[rustfmt::skip]
const OCEAN: [[Oklch; 23]; 2] = [
    [(97.,0.005,195.), (94.5,0.007,195.), (100.,0.,195.), (95.,0.007,195.), (91.5,0.009,195.),
     (21.,0.011,195.), (49.,0.017,195.), (52.5,0.016,195.), (51.,0.088,190.), (98.,0.01,190.),
     (93.,0.039,190.), (95.,0.023,190.), (90.,0.007,195.), (60.,0.014,195.), (21.,0.011,195.),
     (97.,0.005,195.), (64.,0.11,190.), (55.,0.094,190.), (38.,0.074,230.), (55.,0.108,170.),
     (38.,0.064,210.), (25.,0.03,195.), (20.,0.02,195.)],
    [(13.,0.011,195.), (17.,0.012,195.), (19.5,0.012,195.), (25.,0.014,195.), (31.,0.016,195.),
     (96.,0.006,195.), (76.,0.017,195.), (70.,0.016,195.), (68.,0.116,190.), (17.,0.028,190.),
     (32.,0.055,190.), (26.,0.036,190.), (30.,0.012,195.), (64.,0.014,195.), (93.,0.008,195.),
     (18.,0.012,195.), (50.5,0.086,190.), (52.,0.088,190.), (34.,0.066,230.), (52.,0.102,170.),
     (34.,0.058,210.), (0.,0.,0.), (0.,0.,0.)],
];

#[rustfmt::skip]
const EMBER: [[Oklch; 23]; 2] = [
    [(97.,0.006,60.), (94.5,0.008,60.), (100.,0.,60.), (95.,0.008,60.), (91.5,0.011,60.),
     (21.,0.013,60.), (49.,0.02,60.), (53.,0.018,60.), (52.,0.112,70.), (98.,0.01,70.),
     (93.,0.045,70.), (95.,0.027,70.), (90.,0.008,60.), (60.5,0.017,60.), (21.,0.013,60.),
     (97.,0.006,60.), (64.,0.138,70.), (55.,0.118,70.), (38.,0.082,110.), (55.,0.144,50.),
     (38.,0.076,90.), (25.,0.03,60.), (20.,0.02,60.)],
    [(13.,0.013,60.), (17.,0.014,60.), (19.5,0.014,60.), (25.,0.016,60.), (31.,0.018,60.),
     (96.,0.007,60.), (76.,0.02,60.), (70.,0.018,60.), (69.,0.148,70.), (17.,0.036,70.),
     (32.,0.069,70.), (26.,0.042,70.), (30.,0.014,60.), (64.,0.017,60.), (93.,0.01,60.),
     (18.,0.014,60.), (52.,0.112,70.), (52.,0.112,70.), (34.,0.074,110.), (52.,0.136,50.),
     (34.,0.068,90.), (0.,0.,0.), (0.,0.,0.)],
];

#[rustfmt::skip]
const CONTRAST: [[Oklch; 23]; 2] = [
    [(98.5,0.002,285.), (94.5,0.002,285.), (100.,0.,285.), (95.,0.002,285.), (91.5,0.003,285.),
     (12.,0.004,285.), (34.,0.006,285.), (38.,0.005,285.), (42.5,0.19,285.), (99.,0.004,285.),
     (92.,0.039,285.), (95.,0.024,285.), (90.,0.002,285.), (60.5,0.005,285.), (12.,0.004,285.),
     (99.,0.002,285.), (69.5,0.166,285.), (55.,0.16,285.), (38.,0.12,325.), (55.,0.16,265.),
     (38.,0.12,305.), (25.,0.03,285.), (20.,0.02,285.)],
    [(7.,0.004,285.), (10.,0.004,285.), (12.5,0.004,285.), (18.,0.005,285.), (25.,0.005,285.),
     (99.,0.002,285.), (90.,0.006,285.), (86.,0.005,285.), (79.,0.11,285.), (8.,0.05,285.),
     (30.,0.095,285.), (22.,0.053,285.), (34.,0.004,285.), (64.,0.005,285.), (99.,0.003,285.),
     (8.,0.004,285.), (47.5,0.19,285.), (52.,0.16,285.), (34.,0.12,325.), (52.,0.16,265.),
     (34.,0.12,305.), (0.,0.,0.), (0.,0.,0.)],
];

#[cfg(test)]
mod tests {
    use navo_compare_profile::appearance::PaletteId;
    use sidewise_kit::tokens::{contrast_ratio, relative_luminance};

    use super::{PALETTES, oklch};

    /// High contrast draws a 3 px focus ring; every other palette draws 2 px.
    #[test]
    fn focus_rings_are_two_pixels_and_three_on_high_contrast() {
        for (id, spec) in PALETTES.iter() {
            let want = if *id == PaletteId::Contrast { 3.0 } else { 2.0 };
            assert_eq!(spec.ring_px, want, "{id:?} focus ring");
        }
    }

    /// Every palette's dark surface is darker than its light surface.
    #[test]
    fn dark_surfaces_are_darker_than_light_in_every_palette() {
        for (id, spec) in PALETTES.iter() {
            let dark = relative_luminance(spec.dark.surface);
            let light = relative_luminance(spec.light.surface);
            assert!(dark < light, "{id:?} dark {dark} >= light {light}");
        }
    }

    /// The conversion reproduces the ratios the owner measured in `contrast.md`.
    #[test]
    fn violet_light_matches_the_measured_contrast() {
        let light = &PALETTES[0].1.light;
        assert!((contrast_ratio(light.text, light.surface) - 16.26).abs() < 0.01);
        assert!((contrast_ratio(light.muted, light.surface) - 5.76).abs() < 0.01);
    }

    /// Chroma beyond sRGB converges to the gamut boundary instead of clipping channels.
    #[test]
    fn out_of_gamut_chroma_converges_to_the_boundary() {
        let near = oklch((62.0, 0.3, 285.0));
        let far = oklch((62.0, 0.4, 285.0));
        assert!((near.l - far.l).abs() < 1e-3, "{} vs {}", near.l, far.l);
    }
}

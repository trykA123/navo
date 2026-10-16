use std::time::Duration;

use gpui_kit::{Hsla, px};

use super::MODES;
use crate::tokens::{Motion, Surfaces, Theme, contrast_ratio};

fn named(surfaces: &Surfaces, name: &str) -> Hsla {
    surfaces
        .entries()
        .into_iter()
        .find_map(|(entry, color)| (entry == name).then_some(color))
        .unwrap_or_else(|| panic!("no surface token {name}"))
}

const BEHIND_TEXT: [&str; 9] = [
    "mica_from",
    "mica_to",
    "card",
    "field",
    "keycap_face",
    "pill",
    "job_card",
    "sheet",
    "tile",
];

const UI_PARTS: [(&str, &str); 6] = [
    ("card_ring", "mica_from"),
    ("card_ring", "mica_to"),
    ("card_ring", "card"),
    ("progress_fill", "progress_track"),
    ("progress_fill", "pill"),
    ("keycap_lit", "keycap_face"),
];

#[test]
fn both_built_in_themes_define_the_same_surface_tokens() {
    let names = |theme: Theme| theme.surfaces.entries().map(|(name, _)| name);
    assert_eq!(names(Theme::new(MODES[0])), names(Theme::new(MODES[1])));
}

#[test]
fn every_surface_token_is_opaque_except_the_scrim() {
    for mode in MODES {
        for (name, color) in Theme::new(mode).surfaces.entries() {
            let is_opaque = (color.a - 1.0).abs() < f32::EPSILON;
            assert_eq!(is_opaque, name != "scrim", "{mode:?}: {name} is {color:?}");
            assert!(color.a > 0.0, "{mode:?}: {name} is invisible");
        }
    }
}

#[test]
fn light_and_dark_differ_in_every_surface_token() {
    let light = Theme::new(MODES[0]).surfaces.entries();
    let dark = Theme::new(MODES[1]).surfaces.entries();
    for ((name, light), (_, dark)) in light.iter().zip(dark.iter()) {
        assert_ne!(light, dark, "{name} is the same in both themes");
    }
}

#[test]
fn text_and_muted_text_read_at_four_and_a_half_to_one_on_every_new_surface() {
    for mode in MODES {
        let theme = Theme::new(mode);
        for surface in BEHIND_TEXT {
            let background = named(&theme.surfaces, surface);
            for (label, foreground) in [("text", theme.colors.text), ("muted", theme.colors.muted)]
            {
                let ratio = contrast_ratio(foreground, background);
                assert!(
                    ratio >= 4.5,
                    "{mode:?}: {label} on {surface} is {ratio:.2}:1"
                );
            }
        }
    }
}

#[test]
fn keycap_labels_read_at_four_and_a_half_to_one_lit_and_unlit() {
    for mode in MODES {
        let s = Theme::new(mode).surfaces;
        for (label, face) in [
            (s.keycap_label, s.keycap_face),
            (s.keycap_lit_label, s.keycap_lit),
        ] {
            let ratio = contrast_ratio(label, face);
            assert!(ratio >= 4.5, "{mode:?}: a keycap label is {ratio:.2}:1");
        }
    }
}

#[test]
fn rings_progress_and_lit_keycaps_stand_out_three_to_one() {
    for mode in MODES {
        let s = Theme::new(mode).surfaces;
        for (part, next_to) in UI_PARTS {
            let ratio = contrast_ratio(named(&s, part), named(&s, next_to));
            assert!(
                ratio >= 3.0,
                "{mode:?}: {part} on {next_to} is {ratio:.2}:1"
            );
        }
    }
}

#[test]
fn accent_parts_reuse_the_existing_colour_tokens() {
    for mode in MODES {
        let theme = Theme::new(mode);
        let (s, c) = (theme.surfaces, theme.colors);
        assert_eq!(s.card_ring, c.accent);
        assert_eq!(s.progress_fill, c.accent);
        assert_eq!(s.keycap_lit, c.accent);
        assert_eq!(s.keycap_lit_label, c.background);
        assert_eq!(s.keycap_label, c.text);
    }
}

#[test]
fn card_and_job_card_shadows_are_translucent_layers() {
    for mode in MODES {
        let shadows = Theme::new(mode).shadows;
        for (name, layers) in [("card", &shadows.card), ("job_card", &shadows.job_card)] {
            assert!(!layers.is_empty(), "{mode:?}: {name} has no shadow");
            for layer in layers {
                assert!(
                    layer.color.a > 0.0 && layer.color.a < 1.0,
                    "{mode:?}: {name} shadow alpha is {}",
                    layer.color.a
                );
            }
        }
    }
}

#[test]
fn new_radii_are_card_8_pill_15_keycap_6_sheet_12() {
    let radii = Theme::new(MODES[0]).radii;
    assert_eq!(
        [radii.card, radii.pill, radii.keycap, radii.sheet],
        [px(8.), px(15.), px(6.), px(12.)]
    );
}

#[test]
fn motion_runs_fast_120_base_180_slow_280_ms() {
    let motion = Theme::new(MODES[1]).motion;
    assert_eq!(
        [motion.fast, motion.base, motion.slow],
        [120, 180, 280].map(Duration::from_millis)
    );
}

#[test]
fn reduced_motion_keeps_the_fade_but_drops_the_travel() {
    assert_eq!(Motion::travel(px(6.), false), px(6.));
    assert_eq!(Motion::travel(px(6.), true), px(0.));
}

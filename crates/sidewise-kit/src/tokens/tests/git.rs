use gpui_kit::component::theme::oklch;
use gpui_kit::{Hsla, Rgba};

use super::{MODES, every_theme, themed};
use crate::tokens::{ColorTheme, Mode, contrast_ratio};

fn near(got: Hsla, want: Hsla) -> bool {
    let (got, want) = (Rgba::from(got), Rgba::from(want));
    [got.r - want.r, got.g - want.g, got.b - want.b]
        .iter()
        .all(|gap| gap.abs() < 0.002)
}

#[test]
fn the_default_theme_keeps_its_git_glyph_colours() {
    let light = themed(ColorTheme::OpenSea, Mode::Light).git;
    assert!(near(light.conflict, oklch(0.56, 0.2, 25.)));
    assert!(near(light.modified, oklch(0.56, 0.138, 55.)));
    assert!(near(light.sync, oklch(0.56, 0.128, 240.)));
    assert!(near(light.quiet, oklch(0.568, 0.017, 225.)));
    let dark = themed(ColorTheme::OpenSea, Mode::Dark).git;
    assert!(near(dark.conflict, oklch(0.72, 0.15, 25.)));
    assert!(near(dark.modified, oklch(0.72, 0.15, 55.)));
    assert!(near(dark.sync, oklch(0.72, 0.15, 240.)));
    assert!(near(dark.quiet, oklch(0.634, 0.017, 225.)));
}

#[test]
fn every_colour_theme_sets_its_own_git_colours() {
    for mode in MODES {
        let all: Vec<_> = ColorTheme::ALL
            .into_iter()
            .map(|color_theme| themed(color_theme, mode).git)
            .collect();
        for (at, git) in all.iter().enumerate() {
            assert!(
                all.iter().skip(at + 1).all(|other| other != git),
                "{mode:?} theme {at} repeats another theme's git colours"
            );
        }
    }
}

#[test]
fn git_colours_meet_three_to_one_on_rows_in_every_theme() {
    for (label, theme) in every_theme() {
        let git = theme.git;
        for (ground, row) in [
            ("row", theme.surfaces.card),
            ("selected", theme.colors.selection),
        ] {
            for (name, color) in [
                ("conflict", git.conflict),
                ("modified", git.modified),
                ("sync", git.sync),
                ("quiet", git.quiet),
            ] {
                let ratio = contrast_ratio(color, row);
                assert!(ratio >= 3., "{label}: {name} on {ground} is {ratio:.2}:1");
            }
        }
    }
}

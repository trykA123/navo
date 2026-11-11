//! Tests for the tinted-line syntax resolver: contrast on every wash and the untinted identity.

use super::tint::{TintLook, on_tint};
use super::{SyntaxTheme, roles};
use crate::tokens::{ColorTheme, Mode, Theme, ThemeLook, contrast_ratio};

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];
const MINIMUM_CONTRAST: f32 = 4.5;

fn every_combination(mut check: impl FnMut(SyntaxTheme, Mode, &Theme)) {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            for color_theme in ColorTheme::ALL {
                let look = ThemeLook {
                    color_theme,
                    ..ThemeLook::default()
                };
                check(syntax, mode, &Theme::with_look(mode, look));
            }
        }
    }
}

#[test]
fn every_role_reads_on_every_wash_through_the_resolver() {
    every_combination(|syntax, mode, theme| {
        for role in roles(syntax, mode, &theme.colors) {
            let colour = on_tint(role.name, role.color, true, TintLook { mode, syntax });
            for (tone, diff) in theme.diff.tones() {
                let ratio = contrast_ratio(colour, diff.wash);
                assert!(
                    ratio >= MINIMUM_CONTRAST,
                    "{syntax:?} {mode:?}: {} on the {tone} wash is {ratio:.2}:1",
                    role.name
                );
            }
        }
    });
}

#[test]
fn an_untinted_line_keeps_the_base_colour() {
    every_combination(|syntax, mode, theme| {
        for role in roles(syntax, mode, &theme.colors) {
            assert_eq!(
                on_tint(role.name, role.color, false, TintLook { mode, syntax }),
                role.color,
                "{syntax:?} {mode:?}: {}",
                role.name
            );
        }
    });
}

#[test]
fn comments_on_a_tint_use_the_comment_on_tint_colour() {
    every_combination(|syntax, mode, theme| {
        let expected = super::tint::comment_on_tint(syntax, mode);
        for role in roles(syntax, mode, &theme.colors) {
            let got = on_tint(role.name, role.color, true, TintLook { mode, syntax });
            if role.name.starts_with("comment") {
                assert_eq!(got, expected, "{syntax:?} {mode:?}: {}", role.name);
            }
        }
    });
}

#[test]
fn a_role_that_already_reads_on_every_wash_is_not_changed() {
    every_combination(|syntax, mode, theme| {
        for role in roles(syntax, mode, &theme.colors) {
            let worst = theme
                .diff
                .tones()
                .iter()
                .map(|(_, tone)| contrast_ratio(role.color, tone.wash))
                .fold(f32::INFINITY, f32::min);
            if worst >= MINIMUM_CONTRAST && !role.name.starts_with("comment") {
                assert_eq!(
                    on_tint(role.name, role.color, true, TintLook { mode, syntax }),
                    role.color
                );
            }
        }
    });
}

#[test]
fn a_failing_colour_is_moved_until_it_reads_on_every_wash() {
    for mode in MODES {
        let theme = Theme::new(mode);
        let grey = gpui_kit::hsla(0., 0., 0.55, 1.);
        let moved = on_tint(
            "variable",
            grey,
            true,
            TintLook {
                mode,
                syntax: SyntaxTheme::OxidisedIron,
            },
        );
        assert_ne!(moved, grey, "{mode:?}");
        assert_eq!(moved.h, grey.h);
        for (tone, diff) in theme.diff.tones() {
            let ratio = contrast_ratio(moved, diff.wash);
            assert!(ratio >= MINIMUM_CONTRAST, "{mode:?} {tone}: {ratio:.2}:1");
        }
    }
}

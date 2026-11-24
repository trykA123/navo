//! Contrast proof for the terminal colour group: every scheme keeps text, marks and rings
//! readable, and the published colours return when the lift is off.

use gpui_kit::{Hsla, Rgba};

use super::{ColorTheme, every_theme, themed};
use crate::tokens::{Mode, TerminalColors, TerminalScheme, contrast_ratio};

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];
const TEXT_MIN: f32 = 4.5;

/// Hsla round-trips through OKLCH with a sub-percent error; compare the painted colour.
fn same(left: Hsla, right: Hsla) -> bool {
    let (left, right) = (Rgba::from(left), Rgba::from(right));
    [left.r - right.r, left.g - right.g, left.b - right.b]
        .iter()
        .all(|delta| delta.abs() < 1e-4)
}
const MARK_MIN: f32 = 3.0;

fn assert_text_pairs(label: &str, colors: &TerminalColors) {
    let grounds = [
        ("background", colors.background),
        ("block hover", colors.block_hover),
        ("block current", colors.block_current),
        ("selection", colors.selection),
    ];
    let mut texts: Vec<(String, Hsla)> =
        vec![("text".into(), colors.text), ("muted".into(), colors.muted)];
    for (at, color) in colors.ansi.into_iter().enumerate() {
        texts.push((format!("ansi {at}"), color));
    }
    for (name, text) in texts {
        for (ground_name, ground) in grounds {
            let ratio = contrast_ratio(text, ground);
            assert!(
                ratio >= TEXT_MIN,
                "{label}: {name} on {ground_name} is {ratio:.2}:1"
            );
        }
    }
}

fn assert_marks_and_rings(label: &str, colors: &TerminalColors) {
    for (name, mark) in [
        ("mark ok", colors.mark_ok),
        ("mark fail", colors.mark_fail),
        ("mark run", colors.mark_run),
        ("find ring", colors.find_ring),
        ("find current", colors.find_current),
        ("find match", colors.find_match),
        ("cursor", colors.cursor),
    ] {
        let ratio = contrast_ratio(mark, colors.background);
        assert!(
            ratio >= MARK_MIN,
            "{label}: {name} on background is {ratio:.2}:1"
        );
    }
    let match_text = contrast_ratio(colors.find_match_text, colors.find_match);
    assert!(
        match_text >= TEXT_MIN,
        "{label}: find match text on match is {match_text:.2}:1"
    );
    let chrome = contrast_ratio(colors.text, colors.chrome);
    assert!(
        chrome >= TEXT_MIN,
        "{label}: text on chrome is {chrome:.2}:1"
    );
}

#[test]
fn every_app_theme_keeps_the_follow_navo_group_readable() {
    for (label, theme) in every_theme() {
        let colors = TerminalColors::follow(theme.mode, &theme.colors);
        assert_text_pairs(&label, &colors);
        assert_marks_and_rings(&label, &colors);
    }
}

#[test]
fn every_scheme_keeps_text_marks_and_rings_readable() {
    for scheme in TerminalScheme::ALL {
        for mode in MODES {
            let theme = themed(ColorTheme::OpenSea, mode);
            let colors = TerminalColors::from_scheme(scheme, mode, true, &theme.colors);
            let label = format!("{scheme:?} {mode:?}");
            assert_text_pairs(&label, &colors);
            assert_marks_and_rings(&label, &colors);
        }
    }
}

#[test]
fn the_lift_leaves_no_published_low_contrast_text_behind() {
    let theme = themed(ColorTheme::OpenSea, Mode::Dark);
    let published =
        TerminalColors::from_scheme(TerminalScheme::OneDark, Mode::Dark, false, &theme.colors);
    let lifted =
        TerminalColors::from_scheme(TerminalScheme::OneDark, Mode::Dark, true, &theme.colors);

    let comment = contrast_ratio(published.ansi[8], published.background);
    assert!(
        comment < 3.0,
        "One Dark's published comment grey is {comment:.2}:1"
    );
    assert!(
        contrast_ratio(lifted.ansi[8], lifted.background) >= TEXT_MIN,
        "the lift must bring the comment grey to AA"
    );

    let red = contrast_ratio(published.ansi[1], published.background);
    assert!(red < 4.5, "One Dark's published red is {red:.2}:1");
    assert!(
        contrast_ratio(lifted.ansi[1], lifted.background) >= 4.6,
        "the lift must bring the red to 4.6:1"
    );
}

#[test]
fn follow_navo_seeds_from_the_theme_tokens() {
    for mode in MODES {
        let theme = themed(ColorTheme::OpenSea, mode);
        let colors = TerminalColors::follow(mode, &theme.colors);
        for (name, seeded, token) in [
            (
                "background",
                colors.background,
                theme.colors.terminal_background,
            ),
            ("text", colors.text, theme.colors.terminal_text),
            ("muted", colors.muted, theme.colors.terminal_muted),
            ("cursor", colors.cursor, theme.colors.accent),
            ("selection", colors.selection, theme.colors.selection),
            ("mark ok", colors.mark_ok, theme.colors.muted),
            ("mark fail", colors.mark_fail, theme.colors.mark),
            ("mark run", colors.mark_run, theme.colors.accent),
            ("chrome", colors.chrome, theme.colors.background),
        ] {
            assert!(same(seeded, token), "{name} does not copy its theme token");
        }
    }
}

#[test]
fn the_scheme_ids_are_stable_and_distinct() {
    let ids: Vec<&str> = TerminalScheme::ALL.iter().map(|s| s.id()).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert_eq!(TerminalScheme::ALL.len(), 6);
    assert_eq!(TerminalScheme::Catppuccin.id(), "catppuccin");
}

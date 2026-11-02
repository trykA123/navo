use gpui_kit::Hsla;

use super::{every_theme, themed};
use crate::tokens::syntax::roles;
use crate::tokens::{ColorTheme, Mode, SyntaxTheme, Theme, contrast_ratio, treemap_look};

const THEME_INVARIANT: [(&str, &str); 0] = [];

fn every_token(theme: &Theme) -> Vec<(String, Hsla)> {
    let mut tokens: Vec<(String, Hsla)> = Vec::new();
    let mut add = |name: String, color: Hsla| tokens.push((name, color));
    for (name, color) in theme.colors.entries() {
        add(format!("colors.{name}"), color);
    }
    for (name, color) in theme.surfaces.entries() {
        add(format!("surfaces.{name}"), color);
    }
    for (at, color) in theme.marks.fills.into_iter().enumerate() {
        add(format!("marks.fills.{at}"), color);
    }
    for (at, color) in theme.marks.ticks.into_iter().enumerate() {
        add(format!("marks.ticks.{at}"), color);
    }
    for (name, kind) in theme.mem.kinds() {
        add(format!("mem.{name}.strong"), kind.strong);
        add(format!("mem.{name}.tint"), kind.tint);
        add(format!("mem.{name}.text"), kind.text);
    }
    add("mem.free".into(), theme.mem.free);
    add("mem.free_line".into(), theme.mem.free_line);
    for (name, tone) in theme.diff.tones() {
        add(format!("diff.{name}.text"), tone.text);
        add(format!("diff.{name}.mark"), tone.mark);
        add(format!("diff.{name}.fill"), tone.fill);
        add(format!("diff.{name}.wash"), tone.wash);
    }
    add("diff.gap_hatch".into(), theme.diff.gap_hatch);
    add("shadows.ink".into(), theme.shadows.ink);
    let look = treemap_look(theme.mode);
    add("treemap.fill".into(), look.fill(0., 40.));
    add("treemap.ink".into(), look.ink(40.));
    tokens
}

#[test]
fn every_colour_token_differs_between_light_and_dark_unless_marked_invariant() {
    for color_theme in ColorTheme::ALL {
        let light = every_token(&themed(color_theme, Mode::Light));
        let dark = every_token(&themed(color_theme, Mode::Dark));
        assert_eq!(light.len(), dark.len());
        for ((name, light), (_, dark)) in light.iter().zip(&dark) {
            let invariant = THEME_INVARIANT.iter().any(|(token, _)| token == name);
            assert_eq!(
                light == dark,
                invariant,
                "{color_theme:?}: {name} must differ between light and dark, or be listed as theme-invariant (and then must not differ)"
            );
        }
    }
}

#[test]
fn every_syntax_role_differs_between_light_and_dark() {
    for color_theme in ColorTheme::ALL {
        let light = themed(color_theme, Mode::Light).colors;
        let dark = themed(color_theme, Mode::Dark).colors;
        for syntax in SyntaxTheme::ALL {
            let light = roles(syntax, Mode::Light, &light);
            let dark = roles(syntax, Mode::Dark, &dark);
            for (light, dark) in light.iter().zip(&dark) {
                assert_ne!(
                    light.color, dark.color,
                    "{syntax:?}: {} is the same in both",
                    light.name
                );
            }
        }
    }
}

#[test]
fn diff_text_reads_on_its_wash_and_on_every_theme_background() {
    for (label, theme) in every_theme() {
        for (name, tone) in theme.diff.tones() {
            for (ground_name, ground) in [
                ("wash", tone.wash),
                ("background", theme.colors.background),
                ("surface", theme.colors.surface),
            ] {
                let ratio = contrast_ratio(tone.text, ground);
                assert!(
                    ratio >= 4.5,
                    "{label}: {name} text on {ground_name} is {ratio:.2}:1"
                );
            }
        }
    }
}

#[test]
fn diff_marks_stand_out_three_to_one_from_the_wash_and_the_background() {
    for (label, theme) in every_theme() {
        for (name, tone) in theme.diff.tones() {
            for (ground_name, ground) in
                [("wash", tone.wash), ("background", theme.colors.background)]
            {
                let ratio = contrast_ratio(tone.mark, ground);
                assert!(
                    ratio >= 3.0,
                    "{label}: {name} mark on {ground_name} is {ratio:.2}:1"
                );
            }
        }
    }
}

#[test]
fn diff_fills_keep_ordinary_text_readable() {
    for (label, theme) in every_theme() {
        for (name, tone) in theme.diff.tones() {
            let ratio = contrast_ratio(theme.colors.text, tone.fill);
            assert!(ratio >= 4.5, "{label}: text on {name} fill is {ratio:.2}:1");
        }
    }
}

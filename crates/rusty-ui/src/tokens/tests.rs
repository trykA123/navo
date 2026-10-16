use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{Hsla, Pixels, Rgba, TestAppContext, px};

use super::{Mode, Radii, Spacing, Theme, ThemeChoice, apply, current};

mod surfaces;

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];

fn is_opaque_srgb(color: Hsla) -> bool {
    let rgb = Rgba::from(color);
    [color.h, color.s, color.l, color.a]
        .iter()
        .all(|part| part.is_finite())
        && rgb.a == 1.0
}

fn spacing_sizes(spacing: Spacing) -> Vec<(&'static str, Pixels)> {
    let Spacing {
        xxs,
        xs,
        sm,
        md,
        lg,
        xl,
        xxl,
        icon,
        row_height,
        tab_height,
        path_height,
        header_height,
        status_height,
        fkey_height,
        rail_width,
        rail_item_height,
        drawer_height,
        size_column,
        date_column,
    } = spacing;
    vec![
        ("xxs", xxs),
        ("xs", xs),
        ("sm", sm),
        ("md", md),
        ("lg", lg),
        ("xl", xl),
        ("xxl", xxl),
        ("icon", icon),
        ("row_height", row_height),
        ("tab_height", tab_height),
        ("path_height", path_height),
        ("header_height", header_height),
        ("status_height", status_height),
        ("fkey_height", fkey_height),
        ("rail_width", rail_width),
        ("rail_item_height", rail_item_height),
        ("drawer_height", drawer_height),
        ("size_column", size_column),
        ("date_column", date_column),
    ]
}

fn radius_sizes(radii: Radii) -> Vec<(&'static str, Pixels)> {
    let Radii {
        xs,
        sm,
        md,
        lg,
        card,
        pill,
        full,
        keycap,
        sheet,
        job_card,
    } = radii;
    vec![
        ("radius_xs", xs),
        ("radius_sm", sm),
        ("radius_md", md),
        ("radius_lg", lg),
        ("radius_card", card),
        ("radius_pill", pill),
        ("radius_full", full),
        ("radius_keycap", keycap),
        ("radius_sheet", sheet),
        ("radius_job_card", job_card),
    ]
}

#[test]
fn every_colour_token_is_an_opaque_colour_in_both_themes() {
    let light = Theme::new(Mode::Light).colors.entries();
    let dark = Theme::new(Mode::Dark).colors.entries();
    let names = |entries: &[(&'static str, Hsla)]| {
        entries.iter().map(|(name, _)| *name).collect::<Vec<_>>()
    };
    assert_eq!(names(&light), names(&dark));
    for (name, color) in light.iter().chain(dark.iter()) {
        assert!(is_opaque_srgb(*color), "{name} is {color:?}");
    }
}

#[test]
fn every_size_token_is_positive_in_both_themes() {
    for mode in MODES {
        let theme = Theme::new(mode);
        for (name, size) in spacing_sizes(theme.spacing)
            .into_iter()
            .chain(radius_sizes(theme.radii))
        {
            assert!(size > px(0.), "{mode:?}: {name} is {size:?}");
        }
        let typography = theme.typography;
        for font in [
            &typography.ui_font,
            &typography.numbers_font,
            &typography.mono_font,
        ] {
            assert!(!font.is_empty(), "{mode:?}: a font family is empty");
        }
        for size in [
            typography.body,
            typography.small,
            typography.tiny,
            typography.key,
            typography.line_height,
        ] {
            assert!(size > px(0.), "{mode:?}: a type size is {size:?}");
        }
    }
}

#[test]
fn the_design_b_fonts_are_archivo_and_jetbrains_mono() {
    let typography = Theme::new(Mode::Light).typography;
    assert_eq!(typography.ui_font.as_ref(), "Archivo");
    assert_eq!(typography.numbers_font.as_ref(), "Archivo");
    assert_eq!(typography.mono_font.as_ref(), "JetBrains Mono");
}

#[test]
fn every_text_pair_meets_four_and_a_half_to_one() {
    for mode in MODES {
        let pairs = Theme::new(mode).colors.text_pairs();
        assert_eq!(
            pairs.len(),
            28,
            "{mode:?}: a text pair names an unknown token"
        );
        for pair in pairs {
            assert!(
                pair.ratio >= 4.5,
                "{mode:?}: {} on {} is {:.2}:1",
                pair.foreground,
                pair.background,
                pair.ratio
            );
        }
    }
}

#[test]
fn light_and_dark_differ_in_every_colour() {
    let light = Theme::new(Mode::Light).colors.entries();
    let dark = Theme::new(Mode::Dark).colors.entries();
    for ((name, light), (_, dark)) in light.iter().zip(dark.iter()) {
        assert_ne!(light, dark, "{name} is the same in both themes");
    }
}

#[test]
fn mode_toggles_between_light_and_dark() {
    assert_eq!(Mode::Light.toggled(), Mode::Dark);
    assert_eq!(Mode::Dark.toggled(), Mode::Light);
    assert!(Mode::Dark.is_dark());
    assert!(!Mode::Light.is_dark());
}

#[test]
fn the_theme_choice_cycles_system_light_dark() {
    let order = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];
    for (index, choice) in order.iter().enumerate() {
        assert_eq!(choice.next(), order[(index + 1) % order.len()]);
    }
    assert_eq!(ThemeChoice::default(), ThemeChoice::System);
}

#[test]
fn system_follows_the_window_and_the_others_fix_the_mode() {
    for system in [Mode::Light, Mode::Dark] {
        assert_eq!(ThemeChoice::System.resolve(system), system);
        assert_eq!(ThemeChoice::Light.resolve(system), Mode::Light);
        assert_eq!(ThemeChoice::Dark.resolve(system), Mode::Dark);
    }
}

#[gpui_kit::test]
fn applying_a_theme_sets_the_kit_theme_mode(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply(Mode::Dark, cx);
        assert!(KitTheme::global(cx).mode.is_dark());
        assert_eq!(current(cx).mode, Mode::Dark);
        apply(Mode::Light, cx);
        assert!(!KitTheme::global(cx).mode.is_dark());
        assert_eq!(current(cx).mode, Mode::Light);
    });
}

#[gpui_kit::test]
fn applying_a_theme_maps_our_colours_onto_the_kit(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply(Mode::Dark, cx);
        let ours = Theme::new(Mode::Dark);
        let kit = KitTheme::global(cx);
        assert_eq!(kit.colors.background, ours.colors.background);
        assert_eq!(kit.colors.foreground, ours.colors.text);
        assert_eq!(kit.colors.primary, ours.colors.accent);
        assert_eq!(kit.font_family, ours.typography.ui_font);
        assert_eq!(kit.mono_font_family, ours.typography.mono_font);
    });
}

#[gpui_kit::test]
fn current_falls_back_to_light_before_any_theme_is_applied(cx: &mut TestAppContext) {
    cx.update(|cx| assert_eq!(current(cx).mode, Mode::Light));
}

#[test]
fn text_reads_at_four_and_a_half_to_one_over_every_mark_fill() {
    for mode in MODES {
        let theme = Theme::new(mode);
        for (at, fill) in theme.marks.fills.iter().enumerate() {
            let ratio = super::contrast_ratio(theme.colors.text, *fill);
            assert!(ratio >= 4.5, "{mode:?}: text on mark {at} is {ratio:.2}:1");
        }
    }
}

#[test]
fn mark_ticks_stand_out_three_to_one_from_the_surfaces() {
    for mode in MODES {
        let theme = Theme::new(mode);
        for (at, tick) in theme.marks.ticks.iter().enumerate() {
            for surface in [theme.colors.surface, theme.colors.rail] {
                let ratio = super::contrast_ratio(*tick, surface);
                assert!(ratio >= 3.0, "{mode:?}: tick {at} is {ratio:.2}:1");
            }
        }
    }
}

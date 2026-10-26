use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{Hsla, Pixels, Rgba, TestAppContext, px};

use super::{
    ColorTheme, Density, Mode, Radii, Spacing, Theme, ThemeChoice, ThemeLook, apply, apply_look,
    apply_with, current,
};

mod surfaces;

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];

fn themed(color_theme: ColorTheme, mode: Mode) -> Theme {
    let look = ThemeLook {
        color_theme,
        ..ThemeLook::default()
    };
    Theme::with_look(mode, look)
}

pub(super) fn every_theme() -> Vec<(String, Theme)> {
    ColorTheme::ALL
        .into_iter()
        .flat_map(|color_theme| {
            MODES.into_iter().map(move |mode| {
                (
                    format!("{color_theme:?} {mode:?}"),
                    themed(color_theme, mode),
                )
            })
        })
        .collect()
}

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
    for (label, theme) in every_theme() {
        for (name, color) in theme.colors.entries() {
            assert!(is_opaque_srgb(color), "{label}: {name} is {color:?}");
        }
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
            &typography.brand_font,
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
fn every_theme_shares_the_brand_interface_and_tabular_number_fonts() {
    let expected = Theme::new(Mode::Light).typography;
    for (label, theme) in every_theme() {
        let typography = theme.typography;
        assert_eq!(typography, expected, "{label}");
        assert_eq!(typography.brand_font.as_ref(), "Space Grotesk");
        assert_eq!(typography.ui_font.as_ref(), "Inter");
        assert_eq!(typography.numbers_font.as_ref(), "Inter");
        assert_eq!(
            typography.numbers_font_features.tag_value_list(),
            &[("tnum".to_string(), 1)]
        );
        assert_eq!(typography.mono_font.as_ref(), "JetBrains Mono");
        assert_eq!(typography.body, px(13.));
    }
}

#[gpui_kit::test]
fn size_column_digits_have_equal_advance_with_bundled_inter(cx: &mut TestAppContext) {
    let platform = gpui_kit::platform::current_platform(true);
    let cx =
        TestAppContext::build_with_text_system(cx.dispatcher.clone(), None, platform.text_system());
    cx.update(|cx| {
        crate::assets::load_fonts(cx).expect("bundled fonts load");
        let theme = Theme::new(Mode::Light);
        let typography = theme.typography;
        let text_system = gpui_kit::WindowTextSystem::new(cx.text_system().clone());
        for weight in [gpui_kit::FontWeight::NORMAL, gpui_kit::FontWeight::SEMIBOLD] {
            let mut font = gpui_kit::font(typography.numbers_font.clone());
            font.features = typography.numbers_font_features.clone();
            font.weight = weight;
            let width = |digit: &str| {
                let run = gpui_kit::TextRun {
                    len: digit.len(),
                    font: font.clone(),
                    color: theme.colors.text,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                text_system
                    .layout_line(digit, typography.small, &[run], None)
                    .width
            };
            let expected = width("0");
            assert!(expected > px(0.));
            for digit in ["1", "2", "3", "4", "5", "6", "7", "8", "9"] {
                assert_eq!(width(digit), expected, "{digit} at {weight:?}");
            }
        }
    });
}

#[test]
fn every_text_pair_meets_four_and_a_half_to_one() {
    for (label, theme) in every_theme() {
        let pairs = theme.colors.text_pairs();
        assert_eq!(
            pairs.len(),
            28,
            "{label}: a text pair names an unknown token"
        );
        for pair in pairs {
            assert!(
                pair.ratio >= 4.5,
                "{label}: {} on {} is {:.2}:1",
                pair.foreground,
                pair.background,
                pair.ratio
            );
        }
    }
}

#[test]
fn the_mark_colour_reads_on_selection_background_and_zebra() {
    for (label, theme) in every_theme() {
        let c = theme.colors;
        for (name, ground) in [
            ("selection", c.selection),
            ("background", c.background),
            ("zebra", c.zebra),
        ] {
            let ratio = super::contrast_ratio(c.mark, ground);
            assert!(ratio >= 4.5, "{label}: mark on {name} is {ratio:.2}:1");
        }
    }
}

#[test]
fn ui_parts_stand_out_three_to_one_from_the_main_surfaces() {
    for (label, theme) in every_theme() {
        let c = theme.colors;
        for (name, part) in [("accent", c.accent), ("faint", c.faint)] {
            for surface in [c.background, c.surface] {
                let ratio = super::contrast_ratio(part, surface);
                assert!(ratio >= 3.0, "{label}: {name} is {ratio:.2}:1");
            }
        }
    }
}

#[test]
fn light_and_dark_differ_in_every_colour() {
    for color_theme in ColorTheme::ALL {
        let light = themed(color_theme, Mode::Light).colors.entries();
        let dark = themed(color_theme, Mode::Dark).colors.entries();
        for ((name, light), (_, dark)) in light.iter().zip(dark.iter()) {
            assert_ne!(light, dark, "{color_theme:?}: {name} is the same in both");
        }
    }
}

#[test]
fn open_sea_is_the_default_and_every_family_has_its_own_look() {
    assert_eq!(ColorTheme::default(), ColorTheme::OpenSea);
    assert_eq!(Theme::new(Mode::Light).color_theme, ColorTheme::OpenSea);
    for mode in MODES {
        let looks: Vec<_> = ColorTheme::ALL
            .iter()
            .map(|color_theme| themed(*color_theme, mode).colors)
            .collect();
        for (at, colors) in looks.iter().enumerate() {
            assert!(
                !looks[..at].contains(colors),
                "{mode:?}: family {at} repeats"
            );
        }
    }
}

#[test]
fn the_grain_light_background_is_a_neutral_grey_not_cream() {
    let grain = super::palette::tokens(ColorTheme::Grain, Mode::Light);
    for (name, token) in [("bg", grain.bg), ("surface", grain.surface)] {
        assert!(token.c <= 0.004, "{name} chroma is {}", token.c);
    }
}

#[test]
fn the_toml_names_are_unique() {
    let names: Vec<_> = ColorTheme::ALL.iter().map(|theme| theme.toml()).collect();
    for (at, name) in names.iter().enumerate() {
        assert!(!names[..at].contains(name), "{name} repeats");
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
    for (label, theme) in every_theme() {
        for (at, fill) in theme.marks.fills.iter().enumerate() {
            let ratio = super::contrast_ratio(theme.colors.text, *fill);
            assert!(ratio >= 4.5, "{label}: text on mark {at} is {ratio:.2}:1");
        }
    }
}

#[test]
fn mark_ticks_stand_out_three_to_one_from_the_surfaces() {
    for (label, theme) in every_theme() {
        for (at, tick) in theme.marks.ticks.iter().enumerate() {
            for surface in [theme.colors.surface, theme.colors.rail] {
                let ratio = super::contrast_ratio(*tick, surface);
                assert!(ratio >= 3.0, "{label}: tick {at} is {ratio:.2}:1");
            }
        }
    }
}

#[test]
fn memory_kind_colours_meet_their_contrast_in_both_themes() {
    for (label, theme) in every_theme() {
        let ratio = super::contrast_ratio;
        let colors = &theme.colors;
        let mut pairs = vec![(
            "free_line on background",
            ratio(theme.mem.free_line, colors.background),
            3.0,
        )];
        for (name, kind) in theme.mem.kinds() {
            for bg in [colors.background, colors.surface] {
                pairs.push((name, ratio(kind.strong, bg), 3.0));
            }
            pairs.push((name, ratio(kind.text, colors.background), 4.5));
            pairs.push((name, ratio(kind.text, kind.tint), 4.5));
            pairs.push((name, ratio(colors.text, kind.tint), 4.5));
        }
        for (name, got, need) in pairs {
            assert!(got >= need, "{label}: {name} is {got:.2}:1, needs {need}");
        }
    }
}

#[test]
fn normal_density_keeps_todays_rows() {
    let theme = Theme::new(Mode::Light);
    assert_eq!(theme.density, Density::Normal);
    assert_eq!(theme.rows.height, theme.spacing.row_height);
    assert_eq!(theme.rows.height, px(24.));
    assert_eq!(theme.rows.inset, theme.spacing.lg);
}

#[test]
fn compact_rows_are_shorter_and_comfortable_rows_taller() {
    let rows = |density| Theme::with_density(Mode::Dark, density).rows;
    let (compact, normal, comfortable) = (
        rows(Density::Compact),
        rows(Density::Normal),
        rows(Density::Comfortable),
    );
    assert!(compact.height < normal.height && normal.height < comfortable.height);
    assert!(compact.inset < normal.inset && normal.inset < comfortable.inset);
}

#[test]
fn density_cycles_compact_normal_comfortable() {
    assert_eq!(Density::Compact.next(), Density::Normal);
    assert_eq!(Density::Normal.next(), Density::Comfortable);
    assert_eq!(Density::Comfortable.next(), Density::Compact);
    assert_eq!(Density::Comfortable.label(), "Comfortable");
}

#[gpui_kit::test]
fn changing_the_mode_keeps_the_density(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_with(Mode::Light, Density::Compact, cx);
        apply(Mode::Dark, cx);
        let theme = current(cx);
        assert_eq!(theme.mode, Mode::Dark);
        assert_eq!(theme.density, Density::Compact);
    });
}

#[gpui_kit::test]
fn the_colour_theme_survives_a_mode_change_and_reaches_the_kit(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let look = ThemeLook {
            color_theme: ColorTheme::Grain,
            ..ThemeLook::default()
        };
        apply_look(Mode::Light, look, cx);
        apply(Mode::Dark, cx);
        let ours = themed(ColorTheme::Grain, Mode::Dark);
        assert_eq!(current(cx).color_theme, ColorTheme::Grain);
        assert_eq!(current(cx).colors, ours.colors);
        assert_eq!(
            KitTheme::global(cx).colors.background,
            ours.colors.background
        );
    });
}

#[test]
fn ui_scale_multiplies_dimensions_and_preserves_defaults_at_100_percent() {
    let themes = MODES.into_iter().flat_map(|mode| {
        [Density::Compact, Density::Normal, Density::Comfortable]
            .map(|density| Theme::with_density(mode, density))
    });
    for defaults in themes {
        for percent in [80, 100, 150] {
            let mut scaled = defaults.clone();
            scaled.scale(percent);
            let factor = f32::from(percent) / 100.;
            assert_eq!(scaled.typography.body, defaults.typography.body * factor);
            assert_eq!(scaled.spacing.md, defaults.spacing.md * factor);
            assert_eq!(scaled.spacing.icon, defaults.spacing.icon * factor);
            assert_eq!(scaled.radii.md, defaults.radii.md * factor);
            assert_eq!(scaled.rows.height, defaults.rows.height * factor);
            if percent == 100 {
                assert_eq!(scaled, defaults);
            }
        }
    }
}

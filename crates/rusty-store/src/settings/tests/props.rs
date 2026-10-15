use proptest::collection::btree_map;
use proptest::option;
use proptest::prelude::*;

use super::*;

const TEXT: &str = "[^\\p{Cc}]{0,24}";

fn appearance() -> impl Strategy<Value = Appearance> {
    let mode = prop_oneof![
        Just(ThemeMode::System),
        Just(ThemeMode::Light),
        Just(ThemeMode::Dark)
    ];
    let density = prop_oneof![
        Just(Density::Compact),
        Just(Density::Normal),
        Just(Density::Comfortable)
    ];
    (mode, TEXT, TEXT, TEXT, any::<u16>(), density).prop_map(
        |(mode, light_theme, dark_theme, font, font_size, density)| Appearance {
            mode,
            light_theme,
            dark_theme,
            font,
            font_size,
            density,
        },
    )
}

fn files() -> impl Strategy<Value = Files> {
    let sort = prop_oneof![
        Just(SortKey::Name),
        Just(SortKey::Size),
        Just(SortKey::Date),
        Just(SortKey::Kind)
    ];
    (any::<[bool; 4]>(), sort).prop_map(|([hidden, system, folders, descending], sort)| Files {
        show_hidden: hidden,
        show_system: system,
        folders_first: folders,
        sort,
        sort_descending: descending,
    })
}

fn settings() -> impl Strategy<Value = Settings> {
    let selection = any::<[bool; 2]>().prop_map(|[space, patterns]| SelectionSettings {
        space_measures_folders: space,
        patterns_select_folders: patterns,
    });
    let terminal =
        (TEXT, option::of(TEXT)).prop_map(|(provider, command)| Terminal { provider, command });
    let experimental = btree_map(TEXT, any::<bool>(), 0..4);
    (appearance(), files(), selection, terminal, experimental).prop_map(
        |(appearance, files, selection, terminal, experimental)| Settings {
            appearance,
            files,
            selection,
            terminal,
            experimental,
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn settings_survive_to_toml_then_parse(settings in settings()) {
        let text = to_toml(&settings);
        prop_assert_eq!(parse(&text).unwrap(), (settings, Vec::new()));
    }
}

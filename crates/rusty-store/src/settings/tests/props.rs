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
    let hints = prop_oneof![
        Just(ModifierHints::All),
        Just(ModifierHints::Fkeys),
        Just(ModifierHints::Off)
    ];
    let looks = (mode, TEXT, TEXT, TEXT, any::<u16>(), density);
    (looks, hints, any::<u16>()).prop_map(
        |(
            (mode, light_theme, dark_theme, font, font_size, density),
            modifier_hints,
            modifier_hints_delay_ms,
        )| Appearance {
            mode,
            light_theme,
            dark_theme,
            font,
            font_size,
            density,
            modifier_hints,
            modifier_hints_delay_ms,
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
        hidden_files: if hidden {
            HiddenFiles::Hidden
        } else {
            HiddenFiles::Shown
        },
        show_system: system,
        folders_first: folders,
        sort,
        sort_descending: descending,
    })
}

fn automotive() -> impl Strategy<Value = Automotive> {
    let preset = prop_oneof![
        Just(ViewPreset::Simple),
        Just(ViewPreset::Rich),
        Just(ViewPreset::Custom)
    ];
    (preset, any::<[bool; 4]>()).prop_map(
        |(preset, [inspector, memory_strip, charts, row_bars])| Automotive {
            view: AutomotiveView {
                preset,
                inspector,
                memory_strip,
                charts,
                row_bars,
            },
        },
    )
}

fn viewers() -> impl Strategy<Value = (TextViewer, ImageViewer, Automotive)> {
    let text = any::<[bool; 2]>().prop_map(|[line_numbers, line_ends]| TextViewer {
        view: TextView {
            line_numbers,
            line_ends,
        },
    });
    let image = any::<bool>().prop_map(|details| ImageViewer {
        view: ImageView { details },
    });
    (text, image, automotive())
}

pub(super) fn settings() -> impl Strategy<Value = Settings> {
    let selection = any::<[bool; 2]>().prop_map(|[space, patterns]| SelectionSettings {
        space_measures_folders: space,
        patterns_select_folders: patterns,
    });
    let terminal =
        (TEXT, option::of(TEXT)).prop_map(|(provider, command)| Terminal { provider, command });
    let experimental = btree_map(TEXT, any::<bool>(), 0..4);
    let editor = option::of(TEXT).prop_map(|path| Editor { path });
    (
        appearance(),
        files(),
        selection,
        (terminal, editor, viewers()),
        experimental,
    )
        .prop_map(
            |(
                appearance,
                files,
                selection,
                (terminal, editor, (text, image, automotive)),
                experimental,
            )| Settings {
                appearance,
                files,
                selection,
                terminal,
                editor,
                text,
                image,
                automotive,
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

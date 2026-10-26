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
    let color_theme = prop_oneof![
        Just(ColorTheme::OpenSea),
        Just(ColorTheme::RedBill),
        Just(ColorTheme::BlackCap),
        Just(ColorTheme::FineMesh),
        Just(ColorTheme::Grain),
        Just(ColorTheme::InkAndAmber)
    ];
    let looks = (mode, color_theme, TEXT, TEXT, TEXT, any::<u16>(), density);
    (looks, hints, any::<u16>()).prop_map(
        |(
            (mode, color_theme, light_theme, dark_theme, font, font_size, density),
            modifier_hints,
            modifier_hints_delay_ms,
        )| Appearance {
            mode,
            color_theme,
            light_theme,
            dark_theme,
            font,
            font_size,
            ui_scale: 100,
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
        drop_confirm: DropConfirm::default(),
        rename_log: false,
    })
}

fn automotive() -> impl Strategy<Value = Automotive> {
    let preset = prop_oneof![
        Just(ViewPreset::Simple),
        Just(ViewPreset::Rich),
        Just(ViewPreset::Custom)
    ];
    (preset, any::<[bool; 6]>()).prop_map(
        |(
            preset,
            [
                inspector,
                memory_strip,
                charts,
                row_bars,
                usage_bars,
                who_uses,
            ],
        )| Automotive {
            modules: vec!["Com".to_owned(), "Dio_Cfg".to_owned()],
            view: AutomotiveView {
                preset,
                inspector,
                memory_strip,
                charts,
                row_bars,
                usage_bars,
                who_uses,
            },
        },
    )
}

fn window() -> impl Strategy<Value = WindowSettings> {
    any::<[bool; 12]>().prop_map(|flags| {
        let [
            tabs_with_one,
            path_bar,
            drive_button,
            title_search,
            fkey_bar,
            icons,
            ext_column,
            size_column,
            date_column,
            free_space,
            marked_totals,
            contents,
        ] = flags;
        WindowSettings {
            view: WindowView {
                tabs_with_one,
                path_bar,
                drive_button,
                title_search,
                fkey_bar,
                icons,
                ext_column,
                size_column,
                date_column,
                free_space,
                marked_totals,
                contents,
            },
        }
    })
}

fn viewers() -> impl Strategy<Value = (TextViewer, ImageViewer, Automotive, CopySettings)> {
    let syntax = prop_oneof![
        Just(crate::settings::view::SyntaxThemeChoice::OxidisedIron),
        Just(crate::settings::view::SyntaxThemeChoice::Mocha)
    ];
    let text =
        (any::<[bool; 2]>(), syntax).prop_map(|([line_numbers, line_ends], syntax_theme)| {
            TextViewer {
                view: TextView {
                    line_numbers,
                    line_ends,
                    syntax_theme,
                },
            }
        });
    let image = any::<bool>().prop_map(|details| ImageViewer {
        view: ImageView { details },
    });
    let copy = prop_oneof![Just(CopyMode::Auto), Just(CopyMode::Standard)]
        .prop_map(|mode| CopySettings { mode });
    (text, image, automotive(), copy)
}

fn markdown_and_preview() -> impl Strategy<Value = (MarkdownViewer, PreviewViewer)> {
    let markdown = any::<bool>().prop_map(|preview_split| MarkdownViewer {
        view: MarkdownView { preview_split },
    });
    let preview = any::<[bool; 2]>().prop_map(|[type_bars, largest_files]| PreviewViewer {
        view: PreviewView {
            type_bars,
            largest_files,
        },
    });
    (markdown, preview)
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
        (
            terminal,
            editor,
            window(),
            viewers(),
            markdown_and_preview(),
        ),
        experimental,
    )
        .prop_map(
            |(
                appearance,
                files,
                selection,
                (terminal, editor, window, (text, image, automotive, copy), (markdown, preview)),
                experimental,
            )| Settings {
                appearance,
                files,
                selection,
                terminal,
                editor,
                window,
                text,
                image,
                markdown,
                preview,
                copy,
                automotive,
                search: Search::default(),
                retention: Default::default(),
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

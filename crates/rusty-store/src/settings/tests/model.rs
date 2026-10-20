use super::*;

#[test]
fn appearance_defaults() {
    let a = Settings::default().appearance;
    assert_eq!(a.mode, ThemeMode::System);
    assert_eq!(a.light_theme, "Ferrite Light");
    assert_eq!(a.dark_theme, "Ferrite Dark");
    assert_eq!(a.font, "Inter");
    assert_eq!(a.font_size, 13);
    assert_eq!(a.density, Density::Normal);
    assert_eq!(a.modifier_hints, ModifierHints::All);
    assert_eq!(a.modifier_hints_delay_ms, 400);
}

#[test]
fn modifier_hints_read_their_three_values_and_the_delay() {
    for (text, hints) in [
        ("all", ModifierHints::All),
        ("fkeys", ModifierHints::Fkeys),
        ("off", ModifierHints::Off),
    ] {
        let toml =
            format!("[appearance]\nmodifier-hints = \"{text}\"\nmodifier-hints-delay-ms = 650\n");
        let (settings, unknown) = crate::parse_settings(&toml).unwrap();
        assert!(unknown.is_empty());
        assert_eq!(settings.appearance.modifier_hints, hints);
        assert_eq!(settings.appearance.modifier_hints_delay_ms, 650);
    }
}

#[test]
fn files_defaults() {
    let f = Settings::default().files;
    assert_eq!(f.hidden_files, HiddenFiles::Shown);
    assert!(!f.show_system);
    assert!(f.folders_first);
    assert_eq!(f.sort, SortKey::Name);
    assert!(!f.sort_descending);
    assert_eq!(f.drop_confirm, DropConfirm::Never);
}

#[test]
fn selection_defaults() {
    let s = Settings::default().selection;
    assert!(s.space_measures_folders);
    assert!(!s.patterns_select_folders);
}

#[test]
fn terminal_defaults() {
    let t = Settings::default().terminal;
    assert_eq!(t.provider, "windows-terminal");
    assert_eq!(t.command, None);
}

#[test]
fn experimental_defaults_to_empty() {
    assert!(Settings::default().experimental.is_empty());
}

#[test]
fn no_editor_is_chosen_by_default() {
    assert_eq!(Settings::default().editor.path, None);
}

#[test]
fn a_chosen_editor_round_trips_through_toml() {
    let mut settings = Settings::default();
    settings.editor.path = Some(r"C:\Program Files\Notepad++\notepad++.exe".to_owned());
    let text = crate::settings_to_toml(&settings);
    let (back, unknown) = crate::parse_settings(&text).unwrap();
    assert!(unknown.is_empty());
    assert_eq!(back.editor, settings.editor);
}

#[test]
fn the_automotive_view_starts_simple_with_every_rich_part_off() {
    let view = Settings::default().automotive.view;
    assert_eq!(view.preset, crate::ViewPreset::Simple);
    assert!(!(view.inspector || view.memory_strip || view.charts || view.row_bars));
}

#[test]
fn the_automotive_view_reads_its_own_table() {
    let text = "[automotive.view]\npreset = \"custom\"\ninspector = true\nrow-bars = true\n";
    let (settings, unknown) = crate::parse_settings(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(settings.automotive.view.preset, crate::ViewPreset::Custom);
    assert!(settings.automotive.view.inspector && settings.automotive.view.row_bars);
}

#[test]
fn the_text_view_shows_line_numbers_and_hides_line_ends_by_default() {
    let view = Settings::default().text.view;
    assert!(view.line_numbers);
    assert!(!view.line_ends);
}

#[test]
fn the_syntax_theme_defaults_to_oxidised_iron_and_reads_mocha() {
    assert_eq!(
        Settings::default().text.view.syntax_theme,
        crate::settings::view::SyntaxThemeChoice::OxidisedIron
    );
    let (settings, unknown) =
        crate::parse_settings("[text.view]\nsyntax-theme = \"mocha\"\n").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(
        settings.text.view.syntax_theme,
        crate::settings::view::SyntaxThemeChoice::Mocha
    );
}

#[test]
fn the_image_view_starts_without_details() {
    assert!(!Settings::default().image.view.details);
}

#[test]
fn the_text_and_image_views_read_their_own_tables() {
    let text =
        "[text.view]\nline-numbers = false\nline-ends = true\n\n[image.view]\ndetails = true\n";
    let (settings, unknown) = crate::parse_settings(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(!settings.text.view.line_numbers && settings.text.view.line_ends);
    assert!(settings.image.view.details);
}

#[test]
fn every_window_part_but_the_fkey_bar_is_shown_by_default() {
    let view = Settings::default().window.view;
    assert_eq!(view, crate::WindowView::default());
    assert!(
        view.tabs_with_one
            && view.path_bar
            && view.drive_button
            && view.title_search
            && !view.fkey_bar
            && view.icons
    );
    assert!(view.ext_column && view.size_column && view.date_column);
    assert!(view.free_space && view.marked_totals && view.contents);
}

#[test]
fn the_window_view_reads_its_own_table() {
    let text = "[window.view]\npath-bar = false\next-column = false\ncontents = false\n";
    let (settings, unknown) = crate::parse_settings(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    let view = settings.window.view;
    assert!(!view.path_bar && !view.ext_column && !view.contents);
    assert!(view.tabs_with_one && view.size_column && view.free_space);
}

#[test]
fn copy_mode_defaults_to_auto() {
    assert_eq!(Settings::default().copy.mode, crate::CopyMode::Auto);
}

#[test]
fn copy_mode_reads_both_values() {
    for (text, mode) in [
        ("auto", crate::CopyMode::Auto),
        ("standard", crate::CopyMode::Standard),
    ] {
        let (settings, unknown) =
            crate::parse_settings(&format!("[copy]\nmode = \"{text}\"\n")).unwrap();
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(settings.copy.mode, mode);
    }
}

#[test]
fn an_unknown_copy_mode_is_reported_with_its_place() {
    let error = crate::parse_settings("[copy]\nmode = \"turbo\"\n").unwrap_err();
    assert!(
        matches!(
            error,
            crate::SettingsError::Invalid {
                line: 2,
                column: 8,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn the_markdown_split_is_on_by_default_and_reads_its_own_table() {
    assert!(Settings::default().markdown.view.preview_split);
    let (settings, unknown) =
        crate::parse_settings("[markdown.view]\npreview-split = false\n").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(!settings.markdown.view.preview_split);
}

#[test]
fn the_preview_sheet_shows_type_bars_and_largest_files_by_default() {
    let view = Settings::default().preview.view;
    assert!(view.type_bars && view.largest_files);
    let text = "[preview.view]\nlargest-files = false\n";
    let (settings, unknown) = crate::parse_settings(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(settings.preview.view.type_bars && !settings.preview.view.largest_files);
}

#[test]
fn search_defaults() {
    let search = Settings::default().search;
    assert_eq!(search.index, SearchIndex::Auto);
    assert!(search.drives.is_empty());
    assert!(search.walk_roots.is_empty());
    assert_eq!(
        search.exclude,
        ["$Recycle.Bin", "System Volume Information"]
    );
    assert!(!search.network && !search.removable);
    assert!(search.remember);
}

#[test]
fn search_settings_parse_and_round_trip() {
    for (name, index) in [
        ("auto", SearchIndex::Auto),
        ("service", SearchIndex::Service),
        ("walk", SearchIndex::Walk),
        ("off", SearchIndex::Off),
    ] {
        let text = format!(
            r#"[search]
index = "{name}"
drives = ['C:', 'D:']
walk-roots = ['C:\Users\admin']
exclude = ['cache']
network = true
removable = true
remember = false
"#
        );
        let (settings, unknown) = parse(&text).unwrap();
        assert!(unknown.is_empty(), "{unknown:?}");
        assert_eq!(settings.search.index, index);
        assert_eq!(settings.search.drives, ["C:", "D:"]);
        assert_eq!(settings.search.walk_roots, [r"C:\Users\admin"]);
        assert_eq!(settings.search.exclude, ["cache"]);
        assert!(settings.search.network && settings.search.removable);
        assert!(!settings.search.remember);
        assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
    }
}

#[test]
fn invalid_search_values_keep_defaults_and_report_the_error() {
    for value in [
        "index = 'turbo'",
        "index = false",
        "drives = ['C:', 1]",
        "walk-roots = false",
        "exclude = [1]",
        "network = 'yes'",
        "removable = 1",
        "remember = []",
    ] {
        let dir = tmp();
        let path = settings_file(&dir);
        write(&path, &format!("[search]\n{value}\n"));
        let (store, report) = SettingsStore::load(&path);
        assert_eq!(store.current().search, Search::default());
        assert!(
            matches!(report.error, Some(SettingsError::Invalid { line: 2, .. })),
            "{report:?}"
        );
    }
}

#[test]
fn unknown_search_keys_follow_the_existing_warning_path() {
    let (settings, unknown) = parse("[search]\nremember = false\nfuture = true\n").unwrap();
    assert!(!settings.search.remember);
    assert_eq!(unknown, ["search.future"]);
}

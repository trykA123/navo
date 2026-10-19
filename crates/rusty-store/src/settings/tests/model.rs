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
fn every_window_part_is_shown_by_default() {
    let view = Settings::default().window.view;
    assert_eq!(view, crate::WindowView::default());
    assert!(
        view.tabs_with_one
            && view.path_bar
            && view.drive_button
            && view.title_search
            && view.fkey_bar
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

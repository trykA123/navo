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

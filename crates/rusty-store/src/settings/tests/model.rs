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
}

#[test]
fn files_defaults() {
    let f = Settings::default().files;
    assert!(!f.show_hidden);
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

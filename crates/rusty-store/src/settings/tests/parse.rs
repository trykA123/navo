use super::*;

#[test]
fn automotive_modules_are_a_typed_list() {
    let (settings, unknown) = parse("[automotive]\nmodules = [\"Com\", \"Dio_Cfg\"]\n").unwrap();
    assert!(unknown.is_empty());
    assert_eq!(
        settings.value_at("automotive.modules"),
        Some(SettingValue::List(vec!["Com".into(), "Dio_Cfg".into()]))
    );
    assert!(parse("[automotive]\nmodules = [1]\n").is_err());
}

#[test]
fn an_older_file_without_tray_loads_the_tray_defaults() {
    let (settings, unknown) = parse("[files]\nsort = \"size\"\n").unwrap();
    assert!(unknown.is_empty());
    assert!(!settings.tray.keep_running);
    assert_eq!(settings.tray.summon_key, "Win+Alt+N");
    assert_eq!(
        settings.value_at("tray.keep-running"),
        Some(SettingValue::Bool(false))
    );
    assert_eq!(
        settings.value_at("tray.summon-key"),
        Some(SettingValue::Text("Win+Alt+N".into()))
    );
}

fn parsed(text: &str) -> Settings {
    parse(text).unwrap().0
}

fn unknown(text: &str) -> Vec<String> {
    parse(text).unwrap().1
}

#[test]
fn empty_text_gives_defaults() {
    assert_eq!(parse("").unwrap(), (Settings::default(), Vec::new()));
}

#[test]
fn partial_file_keeps_defaults_for_the_rest() {
    let text = "[appearance]\nfont-size = 15\n\n[files]\nhidden-files = \"hidden\"\n";
    let mut expected = Settings::default();
    expected.appearance.font_size = 15;
    expected.files.hidden_files = HiddenFiles::Hidden;
    assert_eq!(parsed(text), expected);
}

#[test]
fn every_theme_mode_parses() {
    for (name, mode) in [
        ("system", ThemeMode::System),
        ("light", ThemeMode::Light),
        ("dark", ThemeMode::Dark),
    ] {
        let text = format!("[appearance]\nmode = \"{name}\"\n");
        assert_eq!(parsed(&text).appearance.mode, mode, "{name}");
    }
}

#[test]
fn every_color_theme_parses() {
    for (name, theme) in [
        ("open-sea", ColorTheme::OpenSea),
        ("red-bill", ColorTheme::RedBill),
        ("black-cap", ColorTheme::BlackCap),
        ("fine-mesh", ColorTheme::FineMesh),
        ("grain", ColorTheme::Grain),
        ("ink-and-amber", ColorTheme::InkAndAmber),
    ] {
        let text = format!("[appearance]\ncolor-theme = \"{name}\"\n");
        assert_eq!(parsed(&text).appearance.color_theme, theme, "{name}");
    }
}

#[test]
fn an_unknown_color_theme_falls_back_to_open_sea_and_the_rest_loads() {
    let text = "[appearance]\ncolor-theme = \"nope\"\nfont-size = 15\n\n[files]\nhidden-files = \"hidden\"\n";
    let (settings, unknown) = parse(text).unwrap();
    assert_eq!(settings.appearance.color_theme, ColorTheme::OpenSea);
    assert_eq!(settings.appearance.font_size, 15);
    assert_eq!(settings.files.hidden_files, HiddenFiles::Hidden);
    assert!(unknown.is_empty(), "{unknown:?}");
}

#[test]
fn every_density_parses() {
    for (name, density) in [
        ("compact", Density::Compact),
        ("normal", Density::Normal),
        ("comfortable", Density::Comfortable),
    ] {
        let text = format!("[appearance]\ndensity = \"{name}\"\n");
        assert_eq!(parsed(&text).appearance.density, density, "{name}");
    }
}

#[test]
fn every_sort_key_parses() {
    for (name, sort) in [
        ("name", SortKey::Name),
        ("size", SortKey::Size),
        ("date", SortKey::Date),
        ("kind", SortKey::Kind),
    ] {
        let text = format!("[files]\nsort = \"{name}\"\n");
        assert_eq!(parsed(&text).files.sort, sort, "{name}");
    }
}

#[test]
fn experimental_flags_parse() {
    let text = "[experimental]\nfast-icons = true\nnew-grid = false\n";
    let flags = parsed(text).experimental;
    assert_eq!(flags.get("fast-icons"), Some(&true));
    assert_eq!(flags.get("new-grid"), Some(&false));
}

#[test]
fn unknown_keys_are_listed_in_file_order() {
    let text = "colour = \"red\"\n\n[appearance]\nfont = \"Iosevka\"\nzoom = 2\n\n[files]\nwrap = true\n\n[plugins]\nx = 1\n";
    assert_eq!(
        unknown(text),
        ["colour", "appearance.zoom", "files.wrap", "plugins"]
    );
}

#[test]
fn unknown_keys_do_not_change_known_values() {
    let text = "[appearance]\nzoom = 2\nfont = \"Iosevka\"\n";
    assert_eq!(parsed(text).appearance.font, "Iosevka");
}

#[test]
fn syntax_error_reports_line_and_column() {
    let error = parse("[files]\nshow-hidden = yes\n").unwrap_err();
    assert!(invalid_at(2, 15)(&error), "{error:?}");
}

#[test]
fn type_error_reports_line_and_column() {
    let error = parse("# fonts\n[appearance]\nfont-size = \"big\"\n").unwrap_err();
    assert!(invalid_at(3, 13)(&error), "{error:?}");
}

#[test]
fn unknown_enum_value_reports_line_and_column() {
    let error = parse("[appearance]\nmode = \"blue\"\n").unwrap_err();
    assert!(invalid_at(2, 8)(&error), "{error:?}");
}

#[test]
fn default_toml_round_trips_without_unknown_keys() {
    let text = to_toml(&Settings::default());
    assert_eq!(parse(&text).unwrap(), (Settings::default(), Vec::new()));
}

#[test]
fn both_hidden_files_values_parse() {
    for (text, value) in [
        ("shown", HiddenFiles::Shown),
        ("hidden", HiddenFiles::Hidden),
    ] {
        let file = format!("[files]\nhidden-files = \"{text}\"\n");
        assert_eq!(parsed(&file).files.hidden_files, value);
    }
}

#[test]
fn every_drop_confirm_value_parses_and_round_trips() {
    for (text, value) in [
        ("never", DropConfirm::Never),
        ("moves", DropConfirm::Moves),
        ("always", DropConfirm::Always),
    ] {
        let file = format!("[files]\ndrop-confirm = \"{text}\"\n");
        let settings = parsed(&file);
        assert_eq!(settings.files.drop_confirm, value);
        assert_eq!(parsed(&to_toml(&settings)), settings);
    }
}

#[test]
fn the_legacy_show_hidden_key_is_ignored_quietly() {
    let old = "[files]\nshow-hidden = false\n\n[editor]\npath = \"code\"\n";
    let (settings, unknown) = parse(old).unwrap();
    assert_eq!(settings.files.hidden_files, HiddenFiles::Shown);
    assert_eq!(settings.editor.path.as_deref(), Some("code"));
    assert!(unknown.is_empty(), "{unknown:?}");
}

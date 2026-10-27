use super::*;

fn preset(name: &str, tiles: &str) -> String {
    format!("[[layout.presets]]\nname = {name:?}\ntiles = {tiles:?}\n")
}

#[test]
fn two_layout_presets_load_and_round_trip() {
    let text = preset("Columns", "row(50, pane, pane)") + &preset("Rows", "col(50, pane, pane)");
    let (settings, unknown) = parse(&text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(settings.layout.presets.len(), 2);
    assert_eq!(settings.layout.presets[1].name, "Rows");
    assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
    assert!(parse("[layout]\n").unwrap().0.layout.presets.is_empty());
}

#[test]
fn invalid_layout_presets_report_their_line_through_settings_problems() {
    for (text, line) in [
        (preset("Broken", "row(99, pane, pane)"), 3),
        (preset("", "pane"), 2),
        (preset(&"x".repeat(41), "pane"), 2),
        (preset("Same", "pane") + &preset("Same", "pane"), 5),
        (
            (0..21)
                .map(|n| preset(&format!("Preset {n}"), "pane"))
                .collect(),
            62,
        ),
    ] {
        let dir = tmp();
        let path = settings_file(&dir);
        write(&path, &text);
        let (store, report) = SettingsStore::load(&path);
        assert_eq!(store.current(), &Settings::default());
        assert!(
            matches!(report.error, Some(SettingsError::Invalid { line: actual, .. }) if actual == line),
            "{report:?}"
        );
    }
    assert!(parse(&preset(&"é".repeat(40), "pane")).is_ok());
}

#[test]
fn saving_then_removing_a_layout_preset_keeps_the_original_bytes() {
    for original in [
        String::new(),
        "[layout]\npresets = [] # keep\n".to_owned(),
        "[layout]\npresets = [{name='Existing', tiles='pane'}]\n".to_owned(),
        "# Settings\n[files]\nshow-system = true # keep\n".to_owned(),
        "# Settings\n[layout]\n# Keep this comment\n\n[files]\nshow-system = true\n".to_owned(),
        "\u{feff}# Settings\r\n[layout]\r\n\r\n[files]\r\nshow-system = true".to_owned(),
        preset("Existing", "pane") + "\n# Footer\n",
    ] {
        let saved = save_layout_preset(&original, "New", "row(50, pane, pane)").unwrap();
        let settings = parse(&saved).unwrap().0;
        assert_eq!(settings.layout.presets.last().unwrap().name, "New");
        assert_eq!(remove_layout_preset(&saved, "New").unwrap(), original);
    }
}

#[test]
fn layout_preset_edits_refuse_duplicates_overflow_invalid_values_and_missing_names() {
    let original = preset("Existing", "pane");
    assert!(save_layout_preset(&original, "Existing", "pane").is_err());
    let full: String = (0..20)
        .map(|n| preset(&format!("Preset {n}"), "pane"))
        .collect();
    assert!(parse(&full).is_ok());
    assert!(save_layout_preset(&full, "Extra", "pane").is_err());
    assert!(save_layout_preset("", "", "pane").is_err());
    assert!(save_layout_preset("", "Broken", "grid").is_err());
    assert!(remove_layout_preset(&original, "Missing").is_err());
}

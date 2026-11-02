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
fn invalid_layout_presets_are_dropped_with_a_problem_line_and_the_rest_loads() {
    for (bad, line) in [
        (preset("Broken", "row(95, pane, pane)"), 9),
        (preset("", "pane"), 8),
        (preset(&"x".repeat(41), "pane"), 8),
        (preset("Good", "col(50, pane, pane)"), 8),
    ] {
        let text = "[files]\nshow-system = true\n\n".to_owned()
            + &preset("Good", "row(50, pane, pane)")
            + &bad
            + "[retention]\nretain-deletes = true\n";
        let dir = tmp();
        let path = settings_file(&dir);
        write(&path, &text);
        let (store, report) = SettingsStore::load(&path);
        assert_eq!(report.error, None);
        let settings = store.current();
        assert!(settings.files.show_system);
        assert!(settings.retention.delete_policy().is_some());
        assert_eq!(settings.layout.presets.len(), 1);
        assert_eq!(settings.layout.presets[0].tiles, "row(50, pane, pane)");
        assert_eq!(report.unknown_keys.len(), 1, "{:?}", report.unknown_keys);
        let problem = &report.unknown_keys[0];
        assert!(problem.starts_with("layout.presets[2]"), "{problem}");
        assert!(problem.contains(&format!("(line {line})")), "{problem}");
    }
    let many: String = (0..22)
        .map(|n| preset(&format!("Preset {n}"), "pane"))
        .collect();
    let (settings, problems) = parse(&many).unwrap();
    assert_eq!(settings.layout.presets.len(), 20);
    assert_eq!(problems.len(), 2);
    assert!(
        problems[0].starts_with("layout.presets[21]"),
        "{problems:?}"
    );
    assert!(
        parse(&preset(&"é".repeat(40), "pane"))
            .unwrap()
            .1
            .is_empty()
    );
    assert!(parse("[[layout.presets]]\nname = 5\n").is_err());
}

#[test]
fn edits_find_the_right_table_when_an_earlier_entry_was_dropped() {
    let original = preset("Same", "grid")
        + &preset("Same", "pane")
        + &preset("", "pane")
        + &preset("Other", "pane");
    let replaced = replace_layout_preset(&original, "Same", "row(50, pane, pane)").unwrap();
    assert_eq!(
        replaced,
        preset("Same", "grid")
            + &preset("Same", "row(50, pane, pane)")
            + &preset("", "pane")
            + &preset("Other", "pane")
    );
    let removed = remove_layout_preset(&original, "Other").unwrap();
    assert_eq!(
        removed,
        preset("Same", "grid") + &preset("Same", "pane") + &preset("", "pane")
    );
    let removed = remove_layout_preset(&original, "Same").unwrap();
    assert_eq!(
        removed,
        preset("Same", "grid") + &preset("", "pane") + &preset("Other", "pane")
    );
    assert!(remove_layout_preset(&original, "").is_err());
    let saved = save_layout_preset(&original, "New", "pane").unwrap();
    let names: Vec<String> = parse(&saved)
        .unwrap()
        .0
        .layout
        .presets
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["Same", "Other", "New"]);
}

#[test]
fn a_full_list_still_replaces_but_refuses_a_new_name() {
    let full: String = (0..20)
        .map(|n| preset(&format!("Preset {n}"), "pane"))
        .collect();
    let replaced = replace_layout_preset(&full, "Preset 19", "col(50, pane, pane)").unwrap();
    let presets = parse(&replaced).unwrap().0.layout.presets;
    assert_eq!(presets.len(), 20);
    assert_eq!(presets[19].tiles, "col(50, pane, pane)");
    assert!(save_layout_preset(&full, "Preset 20", "pane").is_err());
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

#[test]
fn replacing_a_layout_preset_keeps_its_place_and_comments() {
    let original = "# Settings\n".to_owned()
        + &preset("First", "pane")
        + "[[layout.presets]]\nname = \"Second\"\ntiles = \"pane\" # keep\n"
        + "\n[files]\nshow-system = true # also\n";
    let replaced = replace_layout_preset(&original, "Second", "row(50, pane, pane)").unwrap();
    assert_eq!(
        replaced,
        original.replace(
            "tiles = \"pane\" # keep",
            "tiles = \"row(50, pane, pane)\" # keep"
        )
    );
    let inline = "[layout]\npresets = [{name='A', tiles='pane'}, {name='B', tiles='pane'}]\n";
    let replaced = replace_layout_preset(inline, "A", "col(40, pane, pane)").unwrap();
    let presets = parse(&replaced).unwrap().0.layout.presets;
    assert_eq!(presets[0].tiles, "col(40, pane, pane)");
    assert_eq!(presets[1].name, "B");
    assert!(replace_layout_preset(&original, "Missing", "pane").is_err());
    assert!(replace_layout_preset(&original, "First", "grid").is_err());
}

#[test]
fn a_text_edit_writes_the_file_and_keeps_unrelated_comments() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "# Mine\n[files]\nshow-system = true # keep\n");
    let (mut store, _) = SettingsStore::load(&path);
    let before = store.generation();
    store
        .edit_text(|text| save_layout_preset(text, "Mine", "row(50, pane, pane)"))
        .unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with("# Mine\n[files]\nshow-system = true # keep\n"));
    assert!(saved.contains("[[layout.presets]]\nname = \"Mine\""));
    assert_eq!(store.current().layout.presets[0].name, "Mine");
    assert!(store.generation() > before);
}

#[test]
fn a_failed_text_edit_leaves_the_file_and_the_store_unchanged() {
    let dir = tmp();
    let path = settings_file(&dir);
    let original = preset("Kept", "pane");
    write(&path, &original);
    let (mut store, _) = SettingsStore::load(&path);
    let refused = store.edit_text(|text| save_layout_preset(text, "Kept", "pane"));
    assert_eq!(refused.unwrap_err(), SettingsError::Refused);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();
    let read_only = store.edit_text(|text| save_layout_preset(text, "New", "pane"));
    assert_eq!(read_only.unwrap_err(), SettingsError::ReadOnly);
    #[allow(
        clippy::permissions_set_readonly_false,
        reason = "lets the temp dir clean up"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(store.current().layout.presets.len(), 1);
    write(&path, "[files\n");
    let invalid = store.edit_text(|text| save_layout_preset(text, "New", "pane"));
    assert_eq!(invalid.unwrap_err(), SettingsError::FileInvalid);
    assert_eq!(fs::read_to_string(&path).unwrap(), "[files\n");
}

#[test]
fn a_text_edit_retries_once_when_the_file_changed_under_it() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[files]\nshow-system = true\n");
    let (mut store, _) = SettingsStore::load(&path);
    let mut is_first = true;
    let changer = path.clone();
    store
        .edit_text(|text| {
            if std::mem::take(&mut is_first) {
                write(&changer, "[files]\nshow-system = false # hand\n");
            }
            save_layout_preset(text, "Mine", "pane")
        })
        .unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(
        saved.starts_with("[files]\nshow-system = false # hand\n"),
        "{saved}"
    );
    assert!(saved.contains("name = \"Mine\""));
}

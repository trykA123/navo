use super::*;

#[test]
fn empty_toml_gives_automotive_on_and_terminal_off() {
    let (settings, unknown) = parse("").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(settings.modules.is_on("automotive"));
    assert!(!settings.modules.is_on("terminal"));
    assert!(!settings.modules.is_on("unknown_pack"));

    let defaults = Settings::default();
    assert!(defaults.modules.is_on("automotive"));
    assert!(!defaults.modules.is_on("terminal"));
    assert!(!defaults.modules.is_on("unknown_pack"));
}

#[test]
fn existing_file_without_modules_table_uses_defaults() {
    let (settings, unknown) = parse("[files]\nsort = \"size\"\n").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(settings.modules.is_on("automotive"));
    assert!(!settings.modules.is_on("terminal"));
}

#[test]
fn modules_table_reads_explicit_overrides() {
    let text = "[modules]\nautomotive = false\nterminal = true\n";
    let (settings, unknown) = parse(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(!settings.modules.is_on("automotive"));
    assert!(settings.modules.is_on("terminal"));

    let (partial, unknown) = parse("[modules]\nterminal = true\n").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(partial.modules.is_on("automotive"));
    assert!(partial.modules.is_on("terminal"));
}

#[test]
fn round_trip_preserves_unknown_module_ids() {
    let text = "[modules]\nautomotive = false\nfuture_pack = true\nlegacy_pack = false\n";
    let (settings, unknown) = parse(text).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert!(!settings.modules.is_on("automotive"));
    assert!(!settings.modules.is_on("terminal"));
    assert!(settings.modules.is_on("future_pack"));
    assert!(!settings.modules.is_on("legacy_pack"));

    let saved = to_toml(&settings);
    assert!(saved.contains("future_pack = true\n"), "{saved}");
    assert!(saved.contains("legacy_pack = false\n"), "{saved}");

    let (back, back_unknown) = parse(&saved).unwrap();
    assert!(back_unknown.is_empty(), "{back_unknown:?}");
    assert_eq!(back, settings);
    assert!(back.modules.is_on("future_pack"));
    assert!(!back.modules.is_on("legacy_pack"));
}

#[test]
fn set_updates_flags_and_round_trips() {
    let mut settings = Settings::default();
    settings.modules.set("automotive", false);
    settings.modules.set("terminal", true);
    settings.modules.set("future_pack", true);

    assert!(!settings.modules.is_on("automotive"));
    assert!(settings.modules.is_on("terminal"));
    assert!(settings.modules.is_on("future_pack"));

    let (back, unknown) = parse(&to_toml(&settings)).unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(back, settings);

    settings.modules.set("automotive", true);
    assert!(settings.modules.is_on("automotive"));
}

#[test]
fn store_save_and_complete_preserve_unknown_module_ids() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = "[modules]\nfuture_pack = true\n";
    write(&path, before);

    let after_complete = complete_text(before, &Settings::default()).unwrap().text;
    assert!(
        after_complete.contains("[modules]\nfuture_pack = true\n"),
        "{after_complete}"
    );
    assert!(!after_complete.contains(UNUSED_NOTE), "{after_complete}");

    let (mut store, report) = SettingsStore::load(&path);
    assert!(report.unknown_keys.is_empty(), "{report:?}");
    assert!(store.current().modules.is_on("automotive"));
    assert!(store.current().modules.is_on("future_pack"));

    store
        .save_preserving_edits("modules.automotive", Some(SettingValue::Bool(false)))
        .unwrap();
    let (reloaded, report) = SettingsStore::load(&path);
    assert_eq!(report, LoadReport::default());
    assert!(!reloaded.current().modules.is_on("automotive"));
    assert!(!reloaded.current().modules.is_on("terminal"));
    assert!(reloaded.current().modules.is_on("future_pack"));
}

#[test]
fn invalid_module_value_is_rejected_and_keeps_defaults_on_load() {
    let text = "[modules]\nautomotive = \"yes\"\n";
    let error = parse(text).unwrap_err();
    assert!(invalid_at(2, 14)(&error), "{error:?}");

    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, text);
    let (store, report) = SettingsStore::load(&path);
    assert!(store.current().modules.is_on("automotive"));
    assert!(!store.current().modules.is_on("terminal"));
    assert!(
        report.error.as_ref().is_some_and(invalid_at(2, 14)),
        "{report:?}"
    );
}

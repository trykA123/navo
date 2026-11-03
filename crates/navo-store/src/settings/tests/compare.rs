use super::*;

#[test]
fn an_older_file_without_compare_loads_the_compare_defaults() {
    let (settings, unknown) = parse("[files]\nsort = \"size\"\n").unwrap();
    assert!(unknown.is_empty());
    assert_eq!(settings.compare, Compare::default());
    assert_eq!(settings.compare.git_engine, GitEngine::Git);
    assert!(settings.compare.ignore_list.is_empty());
    assert!(settings.compare.respect_gitignore);
    assert_eq!(settings.compare.memory_budget_mib, 1024);
    assert_eq!(settings.compare.content_check, ContentCheck::Content);
}

#[test]
fn the_compare_keys_read_by_their_dotted_names() {
    let settings = Settings::default();
    let at = |key| settings.value_at(key);
    assert_eq!(at("compare.git-engine"), Some(text("git")));
    assert_eq!(at("compare.ignore-list"), Some(SettingValue::List(vec![])));
    assert_eq!(
        at("compare.respect-gitignore"),
        Some(SettingValue::Bool(true))
    );
    assert_eq!(
        at("compare.memory-budget-mib"),
        Some(SettingValue::Integer(1024))
    );
    assert_eq!(at("compare.content-check"), Some(text("content")));
}

#[test]
fn a_compare_table_round_trips() {
    let source = "[compare]\ngit-engine = \"gix\"\nignore-list = [\"*.o\", \"target/\"]\n\
                  respect-gitignore = false\nmemory-budget-mib = 2048\ncontent-check = \"size-time\"\n";
    let (settings, unknown) = parse(source).unwrap();
    assert!(unknown.is_empty());
    assert_eq!(settings.compare.git_engine, GitEngine::Gix);
    assert_eq!(settings.compare.ignore_list, ["*.o", "target/"]);
    assert!(!settings.compare.respect_gitignore);
    assert_eq!(settings.compare.memory_budget_mib, 2048);
    assert_eq!(settings.compare.content_check, ContentCheck::SizeTime);
    assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
}

#[test]
fn every_git_engine_name_parses() {
    for (name, engine) in [
        ("git", GitEngine::Git),
        ("gix", GitEngine::Gix),
        ("auto", GitEngine::Auto),
    ] {
        let (settings, _) = parse(&format!("[compare]\ngit-engine = \"{name}\"\n")).unwrap();
        assert_eq!(settings.compare.git_engine, engine, "{name}");
    }
}

#[test]
fn an_unknown_git_engine_reads_as_auto() {
    let (settings, unknown) = parse("[compare]\ngit-engine = \"libgit2\"\n").unwrap();
    assert!(unknown.is_empty());
    assert_eq!(settings.compare.git_engine, GitEngine::Auto);
}

#[test]
fn an_unknown_content_check_reads_as_content() {
    let (settings, _) = parse("[compare]\ncontent-check = \"crc\"\n").unwrap();
    assert_eq!(settings.compare.content_check, ContentCheck::Content);
}

#[test]
fn the_memory_budget_clamps_into_its_range() {
    for (given, kept) in [
        (0_i64, 256),
        (255, 256),
        (256, 256),
        (1024, 1024),
        (16_384, 16_384),
        (16_385, 16_384),
        (9_000_000_000, 16_384),
        (-5, 256),
    ] {
        let (settings, _) = parse(&format!("[compare]\nmemory-budget-mib = {given}\n")).unwrap();
        assert_eq!(settings.compare.memory_budget_mib, kept, "{given}");
    }
}

#[test]
fn a_wrong_type_in_compare_is_an_error_with_its_line() {
    let wrong = |text: &str| parse(text).unwrap_err();
    assert!(matches!(
        wrong("[compare]\nrespect-gitignore = \"yes\"\n"),
        SettingsError::Invalid { line: 2, .. }
    ));
    assert!(matches!(
        wrong("[compare]\nignore-list = [1]\n"),
        SettingsError::Invalid { line: 2, .. }
    ));
    assert!(matches!(
        wrong("[compare]\nmemory-budget-mib = \"big\"\n"),
        SettingsError::Invalid { line: 2, .. }
    ));
}

#[test]
fn an_unknown_compare_key_is_reported() {
    let (_, unknown) = parse("[compare]\nspeed = 3\n").unwrap();
    assert_eq!(unknown, ["compare.speed"]);
}

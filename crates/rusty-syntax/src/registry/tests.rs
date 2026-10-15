use std::fs;

use tempfile::TempDir;

use super::*;

const DEMO_TOML: &str = r#"
[[language]]
name = "Demo"
extensions = ["demo"]
case_ignored = false
line_comments = ["//"]
block_comments = []
operators = []
numbers = true
keywords = []
delimiters = []
"#;

const DEMO_UDL: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <UserLang name="Udl Demo" ext="udl" udlVersion="2.1">
        <Settings>
            <Global caseIgnored="no" />
        </Settings>
        <KeywordLists>
            <Keywords name="Comments">00; 01 02 03 04</Keywords>
            <Keywords name="Keywords1">begin end</Keywords>
        </KeywordLists>
    </UserLang>
</NotepadPlus>
"#;

fn write(dir: &TempDir, name: &str, text: &str) {
    fs::write(dir.path().join(name), text).unwrap();
}

fn name_of(lang: Option<&Language>) -> Option<&str> {
    lang.map(|l| l.name.as_str())
}

#[test]
fn for_name_finds_by_extension_ignoring_case() {
    let registry = Registry::builtin();
    assert_eq!(name_of(registry.for_name("Board.OIL")), Some("oil"));
    assert_eq!(name_of(registry.for_name("calib.dcm")), Some("dcm"));
}

#[test]
fn get_finds_a_builtin_by_name() {
    assert_eq!(name_of(Registry::builtin().get("m4")), Some("m4"));
}

#[test]
fn load_dir_reports_a_broken_file_and_loads_the_rest() {
    let dir = TempDir::new().unwrap();
    write(&dir, "demo.toml", DEMO_TOML);
    write(&dir, "demo.xml", DEMO_UDL);
    write(&dir, "broken.xml", "<NotepadPlus><UserLang name=");
    write(&dir, "notes.txt", "not a rules file");
    let mut registry = Registry::builtin();
    let errors = registry.load_dir(dir.path());
    let reported: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(errors.len(), 1, "{reported:?}");
    assert!(reported[0].contains("broken.xml"), "{reported:?}");
    assert_eq!(name_of(registry.get("Demo")), Some("Demo"));
    assert_eq!(name_of(registry.for_name("a.udl")), Some("Udl Demo"));
}

#[test]
fn user_file_with_a_builtin_name_replaces_it() {
    let dir = TempDir::new().unwrap();
    let user_oil = DEMO_TOML
        .replace("\"Demo\"", "\"oil\"")
        .replace("[\"demo\"]", "[\"oil\"]")
        .replace("line_comments = [\"//\"]", "line_comments = [\"--\"]");
    write(&dir, "oil.toml", &user_oil);
    let mut registry = Registry::builtin();
    assert!(registry.load_dir(dir.path()).is_empty());
    let found = registry
        .for_name("Board.oil")
        .map(|l| l.line_comments.clone());
    assert_eq!(found, Some(vec!["--".to_owned()]));
}

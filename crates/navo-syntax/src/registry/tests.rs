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
fn autoconf_files_are_not_lexed_as_m4() {
    let registry = Registry::builtin();
    assert_eq!(name_of(registry.for_name("configure.ac")), None);
    assert_eq!(name_of(registry.for_name("macros.m4")), Some("m4"));
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

/// A small valid UDL with the given language name and extension list.
fn udl(name: &str, ext: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <UserLang name="{name}" ext="{ext}" udlVersion="2.1">
        <Settings><Global caseIgnored="no" /></Settings>
        <KeywordLists><Keywords name="Keywords1">begin end</Keywords></KeywordLists>
    </UserLang>
</NotepadPlus>
"#
    )
}

#[test]
fn load_udl_dir_loads_only_xml_files() {
    let dir = TempDir::new().unwrap();
    write(&dir, "a.xml", &udl("A", "a"));
    write(&dir, "b.toml", DEMO_TOML);
    write(&dir, "c.txt", "not a rules file");
    let mut registry = Registry::builtin();
    assert!(registry.load_udl_dir(dir.path()).is_empty());
    assert_eq!(name_of(registry.get("A")), Some("A"));
    assert_eq!(name_of(registry.get("Demo")), None);
}

#[test]
fn load_udl_dir_reads_a_crlf_udl() {
    let dir = TempDir::new().unwrap();
    write(&dir, "crlf.xml", &DEMO_UDL.replace('\n', "\r\n"));
    let mut registry = Registry::builtin();
    assert!(registry.load_udl_dir(dir.path()).is_empty());
    assert_eq!(name_of(registry.get("Udl Demo")), Some("Udl Demo"));
}

#[test]
fn load_udl_dir_claims_every_extension_word() {
    let dir = TempDir::new().unwrap();
    write(&dir, "multi.xml", &udl("Multi", "foo bar"));
    let mut registry = Registry::builtin();
    assert!(registry.load_udl_dir(dir.path()).is_empty());
    assert_eq!(name_of(registry.for_name("a.foo")), Some("Multi"));
    assert_eq!(name_of(registry.for_name("b.bar")), Some("Multi"));
}

#[test]
fn load_udl_dir_reads_a_unicode_folder_name() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("Prüfung-ü");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("uni.xml"), udl("Unicode", "uni")).unwrap();
    let mut registry = Registry::builtin();
    assert!(registry.load_udl_dir(&folder).is_empty());
    assert_eq!(name_of(registry.get("Unicode")), Some("Unicode"));
}

#[test]
fn load_udl_dir_matches_the_xml_extension_ignoring_ascii_case() {
    let dir = TempDir::new().unwrap();
    write(&dir, "A.XML", &udl("Upper", "up"));
    let mut registry = Registry::builtin();
    assert!(registry.load_udl_dir(dir.path()).is_empty());
    assert_eq!(name_of(registry.get("Upper")), Some("Upper"));
}

#[test]
fn a_file_over_the_size_cap_is_skipped_and_loading_continues() {
    let dir = TempDir::new().unwrap();
    let big = fs::File::create(dir.path().join("a.toml")).unwrap();
    big.set_len(MAX_RULE_FILE_BYTES + 1).unwrap();
    write(&dir, "b.xml", &udl("After", "aft"));
    let mut registry = Registry::builtin();
    let errors = registry.load_dir(dir.path());
    let reported: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(errors.len(), 1, "{reported:?}");
    assert!(reported[0].contains("a.toml"), "{reported:?}");
    assert!(reported[0].contains("too large"), "{reported:?}");
    assert_eq!(name_of(registry.get("After")), Some("After"));
}

#[test]
fn a_file_exactly_at_the_size_cap_is_still_read() {
    let dir = TempDir::new().unwrap();
    let padding = " ".repeat(usize::try_from(MAX_RULE_FILE_BYTES).unwrap() - DEMO_TOML.len());
    write(&dir, "a.toml", &format!("{DEMO_TOML}{padding}"));
    let mut registry = Registry::builtin();
    assert!(registry.load_dir(dir.path()).is_empty());
    assert_eq!(name_of(registry.get("Demo")), Some("Demo"));
}

#[test]
fn a_subfolder_named_like_a_rule_file_is_an_error_and_the_rest_loads() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("x.toml")).unwrap();
    write(&dir, "y.xml", &udl("After", "aft"));
    let mut registry = Registry::builtin();
    let errors = registry.load_dir(dir.path());
    let reported: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(errors.len(), 1, "{reported:?}");
    assert!(reported[0].contains("x.toml"), "{reported:?}");
    assert!(reported[0].contains("not a regular file"), "{reported:?}");
    assert_eq!(name_of(registry.get("After")), Some("After"));
}

#[cfg(unix)]
#[test]
fn a_fifo_named_like_a_rule_file_is_skipped_without_blocking() {
    let dir = TempDir::new().unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(dir.path().join("a.xml"))
        .status()
        .unwrap();
    assert!(made.success());
    write(&dir, "b.toml", DEMO_TOML);
    let mut registry = Registry::builtin();
    let errors = registry.load_dir(dir.path());
    let reported: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(errors.len(), 1, "{reported:?}");
    assert!(reported[0].contains("not a regular file"), "{reported:?}");
    assert_eq!(name_of(registry.get("Demo")), Some("Demo"));
}

#[test]
fn a_reader_that_yields_more_than_the_cap_is_rejected() {
    let over = vec![b'a'; 11];
    let result = read_capped(std::io::Cursor::new(over), 10);
    assert!(result.is_err_and(|e| e.to_string().contains("too large")));
    let at_cap = read_capped(std::io::Cursor::new(vec![b'a'; 10]), 10);
    assert_eq!(at_cap.ok().map(|text| text.len()), Some(10));
}

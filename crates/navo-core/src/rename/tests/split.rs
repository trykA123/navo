use crate::rename::split_name;

#[test]
fn splits_at_the_last_dot() {
    assert_eq!(split_name("report.final.pdf"), ("report.final", "pdf"));
}

#[test]
fn a_name_without_a_dot_has_no_extension() {
    assert_eq!(split_name("Makefile"), ("Makefile", ""));
}

#[test]
fn a_leading_dot_does_not_count() {
    assert_eq!(split_name(".gitignore"), (".gitignore", ""));
}

#[test]
fn a_leading_dot_with_an_extension_splits_after_it() {
    assert_eq!(split_name(".config.json"), (".config", "json"));
}

#[test]
fn a_trailing_dot_gives_an_empty_extension() {
    assert_eq!(split_name("name."), ("name", ""));
}

#[test]
fn multi_byte_names_split_on_character_boundaries() {
    assert_eq!(split_name("café.日本"), ("café", "日本"));
}

use crate::find::{Compiled, FindQuery, NameFilter};

fn filter(mask: &str) -> NameFilter {
    NameFilter::new(mask).unwrap()
}

#[test]
fn masks_split_on_semicolons_and_trim() {
    let names = filter(" *.rs ; *.toml ;; ");
    assert!(names.matches("lib.rs"));
    assert!(names.matches("Cargo.toml"));
    assert!(!names.matches("README.md"));
}

#[test]
fn excludes_win_over_includes() {
    let names = filter("*.rs;!test*");
    assert!(names.matches("lib.rs"));
    assert!(!names.matches("tests.rs"));
    assert!(!names.matches("main.c"));
}

#[test]
fn only_excludes_match_everything_else() {
    let names = filter("!*.tmp;!*.bak");
    assert!(names.matches("notes.txt"));
    assert!(!names.matches("draft.TMP"));
    assert!(!names.matches("old.bak"));
}

#[test]
fn bare_words_match_anywhere_in_the_name() {
    let names = filter("report");
    assert!(names.matches("Q3-Report-final.pdf"));
    assert!(names.matches("report"));
    assert!(!names.matches("summary.pdf"));
    assert!(filter("rep?rt").matches("repart"));
    assert!(!filter("rep?rt").matches("my-report.pdf"));
}

#[test]
fn masks_ignore_case() {
    assert!(filter("*.RS").matches("main.rs"));
    assert!(filter("Main").matches("MAIN.rs"));
}

#[test]
fn regex_is_case_insensitive_on_the_name_only() {
    let names = filter("/^lib.*[.]rs$/");
    assert!(names.matches("LIB_core.RS"));
    assert!(!names.matches("mylib.rs"));
    assert!(filter("/a;b/").matches("xa;by"));
}

#[test]
fn bad_regex_and_bare_bang_are_errors() {
    assert!(NameFilter::new("/(/").is_err());
    assert!(NameFilter::new("!").is_err());
    assert!(NameFilter::new("*.rs;! ").is_err());
}

#[test]
fn empty_parts_are_dropped_and_no_includes_match_all() {
    assert!(filter(" ; ").matches("anything.bin"));
    assert!(filter("").matches("anything.bin"));
}

#[test]
fn compiled_query_carries_the_name_filter() {
    let compiled = Compiled::new(&FindQuery::parse("needle name:*.rs;!test*")).unwrap();
    let names = compiled.name_filter().unwrap();
    assert!(names.matches("lib.rs"));
    assert!(!names.matches("test_lib.rs"));
    assert!(
        Compiled::new(&FindQuery::parse("needle"))
            .unwrap()
            .name_filter()
            .is_none()
    );
}

#[test]
fn bare_exclude_word_drops_names_containing_it() {
    let names = filter("name;!test");
    assert!(names.matches("name.rs"));
    assert!(!names.matches("name_test.rs"));
    assert!(!filter("!test").matches("my-TESTS.txt"));
    assert!(filter("!test").matches("main.rs"));
}

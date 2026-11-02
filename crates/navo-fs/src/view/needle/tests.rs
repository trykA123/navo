use super::{Needle, Scratch};

fn found(needle: &str, name: &str) -> Option<std::ops::Range<usize>> {
    Needle::parse(needle).find(name, &mut Scratch::default())
}

fn starts(needle: &str, name: &str) -> bool {
    Needle::parse(needle).starts(name, &mut Scratch::default())
}

#[test]
fn the_text_matches_anywhere_in_the_name() {
    assert_eq!(found("499", "file-4999"), Some(5..8));
    assert_eq!(found("4999", "file-4999"), Some(5..9));
    assert_eq!(found("nope", "file-4999"), None);
}

#[test]
fn the_match_ignores_case() {
    assert_eq!(found("FILE", "my-File.txt"), Some(3..7));
    assert_eq!(found("ärger", "Ärger.txt"), Some(0..6));
}

#[test]
fn a_caret_keeps_only_names_that_start_with_the_rest() {
    assert_eq!(found("^file", "file-1"), Some(0..4));
    assert_eq!(found("^file", "my-file"), None);
}

#[test]
fn a_star_and_a_question_mark_work_as_wildcards() {
    assert_eq!(found("f*9", "file-4999"), Some(0..7));
    assert_eq!(found("i?e", "file-4999"), Some(1..4));
    assert_eq!(found("*icon", "app-icon.svg"), Some(4..8));
    assert_eq!(found("^a*n", "app-icon.svg"), Some(0..8));
    assert_eq!(found("^p*n", "app-icon.svg"), None);
}

#[test]
fn only_the_first_match_is_reported() {
    assert_eq!(found("a", "banana"), Some(1..2));
    assert_eq!(found("a*a", "banana"), Some(1..4));
}

#[test]
fn a_star_alone_or_a_bare_caret_matches_every_name_without_a_range() {
    assert_eq!(found("*", "file"), Some(0..0));
    assert_eq!(found("^", "file"), Some(0..0));
}

#[test]
fn the_range_counts_bytes_of_the_original_name() {
    assert_eq!(found("x", "İx"), Some(2..3));
    assert_eq!(found("ö", "aöb"), Some(1..3));
}

#[test]
fn starts_tells_names_that_begin_with_the_text() {
    assert!(starts("4999", "4999.txt"));
    assert!(!starts("4999", "file-4999"));
    assert!(starts("f*9", "file-4999"));
    assert!(starts("*icon", "app-icon.svg"));
}

#[test]
fn a_semicolon_separates_alternatives() {
    assert_eq!(found("txt;doc", "b.doc"), Some(2..5));
    assert_eq!(found("txt;doc", "c.png"), None);
    assert_eq!(found("^a;^b", "b.txt"), Some(0..1));
    assert_eq!(found("^a;^b", "ca"), None);
    assert_eq!(found("doc;", "x"), None);
}

#[test]
fn starts_checks_every_alternative() {
    assert!(starts("doc;b", "b.txt"));
    assert!(!starts("doc;txt", "b.txt"));
}

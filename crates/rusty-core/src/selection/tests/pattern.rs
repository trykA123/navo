use std::time::{Duration, Instant};

use super::pat;
use crate::{Pattern, PatternError};

#[test]
fn star_matches_any_run() {
    assert!(pat("a*z").matches("abcz"));
}

#[test]
fn star_matches_nothing() {
    assert!(pat("a*z").matches("az"));
}

#[test]
fn star_alone_matches_every_name() {
    assert!(pat("*").matches("anything.at.all"));
}

#[test]
fn star_needs_the_rest_to_match() {
    assert!(!pat("*.rs").matches("main.rsx"));
}

#[test]
fn question_matches_one_character() {
    assert!(pat("a?c").matches("abc"));
}

#[test]
fn question_does_not_match_nothing() {
    assert!(!pat("a?c").matches("ac"));
}

#[test]
fn question_does_not_match_two_characters() {
    assert!(!pat("a?c").matches("abbc"));
}

#[test]
fn star_and_question_mixed() {
    assert!(pat("?*_v?.*").matches("x-report_v2.pdf"));
}

#[test]
fn star_and_question_mixed_rejects() {
    assert!(!pat("?*_v?.*").matches("_v2.pdf"));
}

#[test]
fn literal_matches_whole_name_only() {
    assert!(!pat("main").matches("main.rs"));
}

#[test]
fn star_dot_star_matches_name_without_dot() {
    assert!(pat("*.*").matches("README"));
}

#[test]
fn star_dot_star_matches_name_with_dot() {
    assert!(pat("*.*").matches("a.b"));
}

#[test]
fn matching_ignores_ascii_case() {
    assert!(pat("*.TXT").matches("a.txt"));
}

#[test]
fn matching_ignores_unicode_case() {
    assert!(pat("Ä*").matches("äpfel"));
}

#[test]
fn matching_ignores_unicode_case_in_the_name() {
    assert!(pat("ä*").matches("ÄPFEL"));
}

#[test]
fn any_item_may_match() {
    assert!(pat("*.rs;*.toml").matches("Cargo.toml"));
}

#[test]
fn no_item_matching_is_no_match() {
    assert!(!pat("*.rs;*.toml").matches("README.md"));
}

#[test]
fn spaces_around_items_are_trimmed() {
    assert!(pat("  *.rs ;  *.toml  ").matches("Cargo.toml"));
}

#[test]
fn spaces_inside_items_are_kept() {
    assert!(pat("my file.*").matches("my file.txt"));
}

#[test]
fn empty_items_are_dropped() {
    assert!(pat(";;*.rs;;").matches("lib.rs"));
}

#[test]
fn empty_text_is_empty() {
    assert_eq!(Pattern::parse(""), Err(PatternError::Empty));
}

#[test]
fn only_separators_and_spaces_is_empty() {
    assert_eq!(Pattern::parse(" ; ;"), Err(PatternError::Empty));
}

#[test]
fn empty_error_has_a_message() {
    assert!(!PatternError::Empty.to_string().is_empty());
}

#[test]
fn as_str_returns_the_text_as_given() {
    assert_eq!(pat(" *.rs; *.toml ").as_str(), " *.rs; *.toml ");
}

#[test]
fn non_ascii_literal_matches() {
    assert!(pat("résumé.*").matches("résumé.pdf"));
}

#[test]
fn emoji_names_match() {
    assert!(pat("*🦀*").matches("ferris🦀crab.png"));
}

#[test]
fn cjk_names_match() {
    assert!(pat("文件?.txt").matches("文件一.txt"));
}

#[test]
fn question_matches_one_multi_byte_character() {
    assert!(pat("?.txt").matches("é.txt"));
}

#[test]
fn question_matches_one_emoji() {
    assert!(pat("?.png").matches("🦀.png"));
}

#[test]
fn question_does_not_match_two_multi_byte_characters() {
    assert!(!pat("?.txt").matches("éé.txt"));
}

#[test]
fn many_stars_finish_quickly() {
    let pattern = pat("*a*a*a*a*a*a*b");
    let name = "a".repeat(10_000);
    let start = Instant::now();
    std::hint::black_box(pattern.matches(&name));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn many_stars_still_reject() {
    assert!(!pat("*a*a*a*a*a*a*b").matches(&"a".repeat(10_000)));
}

#[test]
fn final_sigma_matches_its_own_case() {
    assert!(pat("*Σ").matches("ΑΣ"));
}

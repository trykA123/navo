use std::path::Path;

use super::check::{Source, evaluate};
use super::ftl::defined;
use super::is_test_path;
use super::scan::{is_capitalised_multi_word, scan};
use super::tokens::{Token, tokenize};

fn ftl(text: &str) -> Vec<(String, String)> {
    vec![(String::from("a.ftl"), text.to_owned())]
}

fn source(text: &str) -> Source {
    Source {
        path: String::from("crates/rusty-ui/src/x.rs"),
        text: text.to_owned(),
        in_ratchet: true,
    }
}

fn texts(source: &str) -> Vec<String> {
    scan(source)
        .strings
        .into_iter()
        .map(|found| found.text)
        .collect()
}

#[test]
fn strings_skip_comments_chars_and_lifetimes() {
    let code = "// \"no\"\n/* \"no\" /* \"no\" */ */ fn f<'a>(c: char) { let x = ('\"', '\\'', \"yes\"); }";
    assert_eq!(texts(code), ["yes"]);
}

#[test]
fn raw_strings_are_read_and_byte_strings_are_ignored() {
    assert_eq!(
        texts("let a = r#\"Two \"words\"\"#; let b = b\"No No\";"),
        ["Two \"words\""]
    );
}

#[test]
fn escapes_become_text() {
    assert_eq!(
        texts("\"Line one\\nLine \\\"two\\\"\""),
        ["Line one Line \"two\""]
    );
}

#[test]
fn line_numbers_follow_newlines() {
    let tokens = tokenize("a\n\n\"b\"");
    assert_eq!(tokens[1].token, Token::Str(String::from("b")));
    assert_eq!(tokens[1].line, 3);
}

#[test]
fn calls_find_literal_ids_in_the_macro_and_the_functions() {
    let code = "let a = t!(\"one\"); let b = tr(\"two\"); let c = rusty_l10n::tr_args(\"three\", &x); \
                let d = t!(\"four\", n = 1); let e = tr(name); let f = other(\"five\");";
    let ids: Vec<String> = scan(code)
        .calls
        .into_iter()
        .map(|found| found.text)
        .collect();
    assert_eq!(ids, ["one", "two", "three", "four"]);
}

#[test]
fn test_items_and_attributes_are_not_scanned() {
    let code = "#[expect(clippy::x, reason = \"Reason with words\")]\nfn live() { f(\"Live text\"); }\n\
                #[cfg(test)]\nmod tests { fn t() { f(\"Test text\"); } }\n\
                #[cfg(test)]\nuse a::{b, c};\n\
                struct S { #[cfg(test)] seen: [u8; 2], real: u8 }\n\
                #[test]\nfn one() { f(\"More test\"); }\n\
                fn last() { f(\"Last text\"); }";
    assert_eq!(texts(code), ["Live text", "Last text"]);
}

#[test]
fn a_test_expression_ends_at_the_closing_brace() {
    let code =
        "fn f() -> bool { #[cfg(test)] self.go(\"Hidden words\") }\nfn g() { h(\"Shown words\"); }";
    assert_eq!(texts(code), ["Shown words"]);
}

#[test]
fn only_capitalised_multi_word_texts_count() {
    assert!(is_capitalised_multi_word("Not found"));
    assert!(is_capitalised_multi_word("Copy  to clipboard"));
    assert!(!is_capitalised_multi_word("Cancel"));
    assert!(!is_capitalised_multi_word("not found"));
    assert!(!is_capitalised_multi_word("{} items"));
    assert!(!is_capitalised_multi_word(""));
}

#[test]
fn test_paths_are_recognised() {
    for yes in [
        "crates/a/src/tests.rs",
        "crates/a/src/tests/x.rs",
        "crates/a/src/test_window.rs",
        "crates/a/src/state_tests.rs",
        "crates/a/src/paged/tests_paths.rs",
    ] {
        assert!(is_test_path(Path::new(yes)), "{yes}");
    }
    for no in [
        "crates/a/src/latest.rs",
        "crates/a/src/contest/x.rs",
        "crates/a/src/lib.rs",
    ] {
        assert!(!is_test_path(Path::new(no)), "{no}");
    }
}

#[test]
fn ftl_reports_parse_errors_with_file_and_line() {
    let known = defined(&ftl("good = Yes\n\nbroken\n"));
    assert_eq!(known.problems.len(), 1);
    assert!(
        known.problems[0].starts_with("a.ftl:3:"),
        "{:?}",
        known.problems
    );
}

#[test]
fn ftl_reports_ids_defined_twice() {
    let files = vec![
        (String::from("a.ftl"), String::from("same = 1\n")),
        (String::from("b.ftl"), String::from("same = 2\n")),
    ];
    let known = defined(&files);
    assert_eq!(known.problems, ["b.ftl: same is already defined in a.ftl"]);
}

#[test]
fn a_used_known_id_with_an_exact_ratchet_passes() {
    let outcome = evaluate(
        &ftl("files-open = Open\n"),
        &[source("t!(\"files-open\");")],
        "0\n",
    );
    assert_eq!(outcome.errors, Vec::<String>::new());
}

#[test]
fn a_literal_id_that_is_not_defined_fails_with_its_place() {
    let outcome = evaluate(
        &ftl("files-open = Open\n"),
        &[source("\n\ntr(\"files-close\"); \"files-open\";")],
        "0",
    );
    assert_eq!(
        outcome.errors,
        ["crates/rusty-ui/src/x.rs:3: unknown message id files-close"]
    );
}

#[test]
fn an_id_that_no_code_uses_fails_except_command_ids() {
    let outcome = evaluate(
        &ftl("files-open = Open\ncmd-tab-new = New tab\n"),
        &[source("fn f() {}")],
        "0",
    );
    assert_eq!(outcome.errors, ["a.ftl: files-open is never used"]);
}

#[test]
fn an_id_held_in_a_plain_literal_counts_as_used() {
    let outcome = evaluate(
        &ftl("files-open = Open\n"),
        &[source("const ID: &str = \"files-open\";")],
        "0",
    );
    assert_eq!(outcome.errors, Vec::<String>::new());
}

#[test]
fn more_hard_coded_texts_than_allowed_fail() {
    let outcome = evaluate(
        &ftl(""),
        &[source("f(\"Not found\"); f(\"Open file\");")],
        "1",
    );
    assert_eq!(outcome.literals.len(), 2);
    assert_eq!(outcome.errors.len(), 1);
    assert!(outcome.errors[0].contains("2 hard-coded texts, 1 allowed: move"));
}

#[test]
fn fewer_hard_coded_texts_than_allowed_ask_to_lower_the_file() {
    let outcome = evaluate(&ftl(""), &[source("f(\"Not found\");")], "3");
    assert_eq!(
        outcome.errors,
        ["1 hard-coded texts, 3 allowed: lower xtask/l10n-allow.txt to 1"]
    );
}

#[test]
fn files_outside_the_ratchet_are_not_counted() {
    let mut other = source("f(\"Not found\");");
    other.in_ratchet = false;
    let outcome = evaluate(&ftl(""), &[other], "0");
    assert_eq!(outcome.errors, Vec::<String>::new());
}

#[test]
fn an_unreadable_allow_file_fails() {
    let outcome = evaluate(&ftl(""), &[], "many");
    assert_eq!(
        outcome.errors,
        ["xtask/l10n-allow.txt must hold one number"]
    );
}

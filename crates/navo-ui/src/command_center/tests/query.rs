use proptest::prelude::*;

use crate::command_center::{Query, Scope, Term};

fn name(text: &str) -> Term {
    Term::Name(text.to_owned())
}

fn path(text: &str) -> Term {
    Term::Path(text.to_owned())
}

fn ext(list: &[&str]) -> Term {
    Term::Ext(list.iter().map(|ext| (*ext).to_owned()).collect())
}

#[test]
fn empty_input_is_a_blank_mixed_query() {
    let query = Query::parse("");
    assert_eq!(query.scope, Scope::Mixed);
    assert!(query.terms.is_empty());
    assert_eq!(query.path, None);
    assert!(query.is_blank());
}

#[test]
fn plain_words_are_name_terms_in_mixed_scope() {
    let query = Query::parse("  read   me ");
    assert_eq!(query.scope, Scope::Mixed);
    assert_eq!(query.terms, vec![name("read"), name("me")]);
    assert!(!query.is_blank());
}

#[test]
fn greater_than_scopes_to_commands() {
    let query = Query::parse(">toggle hidden");
    assert_eq!(query.scope, Scope::Commands);
    assert_eq!(query.terms, vec![name("toggle"), name("hidden")]);
}

#[test]
fn greater_than_with_space_and_alone() {
    assert_eq!(Query::parse("> copy").terms, vec![name("copy")]);
    let alone = Query::parse(">");
    assert_eq!(alone.scope, Scope::Commands);
    assert!(alone.is_blank());
}

#[test]
fn slash_scopes_to_files() {
    let query = Query::parse("/notes todo");
    assert_eq!(query.scope, Scope::Files);
    assert_eq!(query.terms, vec![name("notes"), name("todo")]);
    assert_eq!(query.path, None);
}

#[test]
fn prefix_only_counts_as_the_first_character() {
    let query = Query::parse(" >copy");
    assert_eq!(query.scope, Scope::Mixed);
    assert_eq!(query.terms, vec![name(">copy")]);
    assert_eq!(Query::parse("a>b").scope, Scope::Mixed);
}

#[test]
fn drive_letter_starts_path_mode() {
    for input in ["C:", "c:\\", r"D:\Program Files\App", "e:foo"] {
        let query = Query::parse(input);
        assert_eq!(query.scope, Scope::Files, "{input}");
        assert_eq!(query.path.as_deref(), Some(input), "{input}");
        assert!(query.terms.is_empty(), "{input}");
        assert!(!query.is_blank(), "{input}");
    }
}

#[test]
fn unc_prefix_starts_path_mode() {
    let query = Query::parse(r"\\srv\share\docs");
    assert_eq!(query.scope, Scope::Files);
    assert_eq!(query.path.as_deref(), Some(r"\\srv\share\docs"));
}

#[test]
fn single_backslash_is_not_path_mode() {
    let query = Query::parse(r"\srv");
    assert_eq!(query.path, None);
    assert_eq!(query.terms, vec![path(r"\srv")]);
}

#[test]
fn tilde_starts_path_mode() {
    for input in ["~", "~/projects", r"~\Documents"] {
        let query = Query::parse(input);
        assert_eq!(query.scope, Scope::Files, "{input}");
        assert_eq!(query.path.as_deref(), Some(input), "{input}");
    }
}

#[test]
fn tilde_in_a_file_name_is_not_path_mode() {
    let query = Query::parse("~$report");
    assert_eq!(query.scope, Scope::Mixed);
    assert_eq!(query.path, None);
    assert_eq!(query.terms, vec![name("~$report")]);
}

#[test]
fn a_letter_colon_inside_a_word_is_not_path_mode() {
    assert_eq!(Query::parse("note: a").path, None);
    assert_eq!(Query::parse("1:2").path, None);
}

#[test]
fn linux_absolute_path_after_slash_is_a_path_term_in_files_mode() {
    let query = Query::parse("/x/y");
    assert_eq!(query.scope, Scope::Files);
    assert_eq!(query.path, None);
    assert_eq!(query.terms, vec![path("x/y")]);
}

#[test]
fn double_slash_keeps_the_leading_separator() {
    let query = Query::parse("//home/me");
    assert_eq!(query.scope, Scope::Files);
    assert_eq!(query.terms, vec![path("/home/me")]);
}

#[test]
fn a_term_with_a_separator_is_a_path_term() {
    let query = Query::parse(r"src\ui main ui/tests");
    assert_eq!(
        query.terms,
        vec![path(r"src\ui"), name("main"), path("ui/tests")]
    );
}

#[test]
fn ext_lists_split_on_commas() {
    let query = Query::parse("ext:rs,toml config");
    assert_eq!(query.terms, vec![ext(&["rs", "toml"]), name("config")]);
}

#[test]
fn ext_lists_drop_dots_empty_items_and_case() {
    let query = Query::parse("EXT:.RS,,.Toml,");
    assert_eq!(query.terms, vec![ext(&["rs", "toml"])]);
}

#[test]
fn empty_ext_term_is_ignored_while_typing() {
    let query = Query::parse("ext: ext:, main");
    assert_eq!(query.terms, vec![name("main")]);
}

#[test]
fn ext_works_in_every_scope() {
    assert_eq!(Query::parse("/ext:png").terms, vec![ext(&["png"])]);
    assert_eq!(Query::parse(">ext:png").terms, vec![ext(&["png"])]);
}

#[test]
fn multi_byte_input_does_not_panic() {
    let query = Query::parse("é ext:ß,Ünï / 日本語/ファイル");
    assert_eq!(
        query.terms,
        vec![
            name("é"),
            ext(&["ß", "ünï"]),
            path("/"),
            path("日本語/ファイル")
        ]
    );
    assert_eq!(Query::parse("/日本").terms, vec![name("日本")]);
}

fn terms_hold_no_whitespace(query: &Query) -> bool {
    query.terms.iter().all(|term| match term {
        Term::Name(text) | Term::Path(text) => {
            !text.is_empty() && !text.chars().any(char::is_whitespace)
        }
        Term::Ext(list) => !list.is_empty() && list.iter().all(|ext| !ext.is_empty()),
    })
}

proptest! {
    #[test]
    fn parse_never_panics(input in "\\PC{0,40}") {
        let _ = Query::parse(&input);
    }

    #[test]
    fn terms_are_never_empty_or_spaced(input in "[ a-zA-Z0-9:,./\\\\~>é]{0,40}") {
        prop_assert!(terms_hold_no_whitespace(&Query::parse(&input)));
    }

    #[test]
    fn path_mode_never_carries_terms(input in "[a-zA-Z]:[ -~]{0,20}|\\\\\\\\[ -~]{0,20}|~[/\\\\][ -~]{0,20}") {
        let query = Query::parse(&input);
        prop_assert_eq!(query.scope, Scope::Files);
        prop_assert_eq!(query.path.as_deref(), Some(input.as_str()));
        prop_assert!(query.terms.is_empty());
    }

    #[test]
    fn only_separator_words_become_path_terms(input in "[ a-z0-9./\\\\]{0,40}") {
        for term in Query::parse(&input).terms {
            match term {
                Term::Path(text) => prop_assert!(text.contains(['/', '\\'])),
                Term::Name(text) => prop_assert!(!text.contains(['/', '\\'])),
                Term::Ext(_) => {}
            }
        }
    }

    #[test]
    fn scope_follows_the_first_character(rest in "[a-z ]{0,20}") {
        prop_assert_eq!(Query::parse(&format!(">{rest}")).scope, Scope::Commands);
        prop_assert_eq!(Query::parse(&format!("/{rest}")).scope, Scope::Files);
        prop_assert_eq!(Query::parse(&format!("x{rest}")).scope, Scope::Mixed);
    }
}

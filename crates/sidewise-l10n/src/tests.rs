//! Tests for the copied loader and the bundled compare messages.

use std::collections::BTreeSet;

use crate::global::Reported;
use crate::{Catalog, CatalogError, FILES, FluentArgs, LookupError, catalog, tr, tr_args};

pub(crate) const FIXTURE: &str = "\
fixture-greeting = Hello, { $name }!
fixture-files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
fixture-attributes-only =
    .title = Only an attribute
";

fn fixture() -> Catalog {
    let (catalog, errors) = Catalog::build(&[("fixture.ftl", FIXTURE)]);
    assert!(errors.is_empty(), "{errors:?}");
    catalog
}

#[test]
fn embedded_files_build_without_errors() {
    let (_, errors) = Catalog::build(FILES);
    assert_eq!(errors, Vec::new());
}

#[test]
fn embedded_file_list_matches_the_locale_directory() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales/en-US");
    let on_disk: BTreeSet<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    let embedded: BTreeSet<String> = FILES.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(embedded, on_disk);
}

#[test]
fn a_compare_message_comes_from_the_embedded_file() {
    assert_eq!(
        tr("compare-could-not-start"),
        "The compare could not start."
    );
    assert!(catalog().has("compare-could-not-start"));
}

#[test]
fn the_update_strings_are_embedded() {
    let ids = [
        "update-line-off",
        "update-line-on",
        "update-turn-on",
        "update-turn-off",
        "update-prompt-title",
        "update-prompt-body",
        "update-not-now",
        "update-available",
        "update-release-page",
        "update-hide",
    ];
    for id in ids {
        assert!(catalog().has(id), "{id} is missing");
    }
}

#[test]
fn the_update_notice_names_the_version() {
    assert_eq!(
        t!("update-available", version = "0.1.0-alpha.3"),
        "Sidewise 0.1.0-alpha.3 is available."
    );
}

#[test]
fn the_git_strings_are_embedded() {
    let ids = [
        "git-chip-revision",
        "git-chip-not-repository",
        "git-chip-no-revision",
        "git-chip-unsupported",
        "git-chip-broken",
        "git-start-tip",
        "git-side-no-file",
    ];
    for id in ids {
        assert!(catalog().has(id), "{id} is missing");
    }
}

#[test]
fn the_git_remote_strings_are_embedded() {
    let ids = [
        "git-remote-chip-revision",
        "git-remote-chip-bad-address",
        "git-remote-unreachable",
        "git-remote-not-found",
        "git-remote-redirected",
        "git-remote-no-such-revision",
        "git-remote-too-large",
        "git-remote-object-too-large",
        "git-remote-timed-out",
        "git-remote-broken",
        "git-remote-cli-refused",
    ];
    for id in ids {
        assert!(catalog().has(id), "{id} is missing");
    }
}

#[test]
fn the_git_remote_not_found_text_is_exact() {
    assert_eq!(
        t!("git-remote-not-found"),
        "Repository not found, or it needs a sign-in (not supported yet)"
    );
}

#[test]
fn the_git_side_without_a_file_names_the_revision() {
    assert_eq!(
        t!("git-side-no-file", rev = "main"),
        "This side is Git revision main; it has no file on disk."
    );
}

#[test]
fn the_macro_passes_named_arguments() {
    assert_eq!(t!("fixture-greeting", name = "Ana"), "Hello, Ana!");
}

#[test]
fn arguments_are_not_wrapped_in_isolation_marks() {
    let text = t!("fixture-greeting", name = "Ana");
    assert!(!text.contains(['\u{2068}', '\u{2069}']));
}

#[test]
fn the_function_form_takes_prepared_arguments() {
    let mut args = FluentArgs::new();
    args.set("name", String::from("Bo"));
    assert_eq!(tr_args("fixture-greeting", &args), "Hello, Bo!");
}

#[test]
fn counts_select_the_english_plural_form() {
    assert_eq!(t!("fixture-files", count = 1), "1 file");
    assert_eq!(t!("fixture-files", count = 0), "0 files");
    assert_eq!(t!("fixture-files", count = 2), "2 files");
}

#[test]
fn a_missing_id_is_a_lookup_error() {
    assert_eq!(
        fixture().lookup("no-such-id", None),
        Err(LookupError::Missing)
    );
}

#[test]
fn a_message_with_attributes_only_has_no_value() {
    assert_eq!(
        fixture().lookup("fixture-attributes-only", None),
        Err(LookupError::NoValue)
    );
}

#[test]
fn a_missing_argument_is_a_format_error() {
    assert!(matches!(
        fixture().lookup("fixture-greeting", None),
        Err(LookupError::Format(_))
    ));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "l10n lookup of no-such-id failed")]
fn a_missing_id_fails_the_debug_assertion() {
    let _ = tr("no-such-id");
}

#[cfg(not(debug_assertions))]
#[test]
fn a_missing_id_shows_the_id() {
    assert_eq!(tr("no-such-id"), "no-such-id");
}

#[test]
fn a_failed_id_is_reported_once() {
    let reported = Reported::default();
    assert!(reported.first("no-such-id"));
    assert!(!reported.first("no-such-id"));
    assert!(reported.first("other-id"));
}

#[test]
fn a_file_that_does_not_parse_is_reported_by_name() {
    let (_, errors) = Catalog::build(&[("bad.ftl", "= no id\n")]);
    assert!(matches!(
        errors.as_slice(),
        [CatalogError::Parse { file, .. }] if file == "bad.ftl"
    ));
}

#[test]
fn a_message_defined_twice_is_reported() {
    let (_, errors) = Catalog::build(&[("one.ftl", "same = 1\n"), ("two.ftl", "same = 2\n")]);
    assert!(matches!(
        errors.as_slice(),
        [CatalogError::Redefined { file, .. }] if file == "two.ftl"
    ));
}

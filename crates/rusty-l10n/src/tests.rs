use std::collections::BTreeSet;

use crate::global::Reported;
use crate::{
    Catalog, CatalogError, FILES, FluentArgs, LookupError, Platform, catalog, tr, tr_args,
};

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
    let (_, errors) = Catalog::build(&FILES);
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
fn a_plain_message_comes_from_the_embedded_files() {
    assert_eq!(tr("cmd-app-quit"), "Quit");
    assert!(catalog().has("cmd-app-quit"));
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

fn embedded_on(platform: Platform) -> Catalog {
    let (catalog, errors) = Catalog::build_on(&FILES, platform);
    assert_eq!(errors, Vec::new());
    catalog
}

#[test]
fn delete_texts_name_the_recycle_bin_on_windows_and_the_trash_elsewhere() {
    for (platform, bin) in [
        (Platform::Windows, "Recycle Bin"),
        (Platform::Other, "Trash"),
    ] {
        let catalog = embedded_on(platform);
        let text = |id| catalog.lookup(id, None).unwrap();
        assert_eq!(text("cmd-files-recycle"), format!("Move to {bin}"));
        let mut args = FluentArgs::new();
        args.set("from", String::from("docs"));
        let route = catalog.lookup("job-route-recycle", Some(&args)).unwrap();
        assert_eq!(route, format!("docs → {bin}"));
        args.set("items", String::from("2 items"));
        let title = catalog
            .lookup("files-recycle-confirm", Some(&args))
            .unwrap();
        assert_eq!(title, format!("Move 2 items to the {bin}?"));
    }
}

#[test]
fn the_current_platform_picks_the_catalog_wording() {
    let bin = if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    };
    assert_eq!(tr("cmd-files-recycle"), format!("Move to {bin}"));
}

use rusty_core::rename::check::{Existing, ExistingName, Fold, Problem, RowState};
use rusty_core::rename::{Find, NameCase, RenameRules};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::multi_rename::{Form, Item, Listing, NameCount, Preview};

pub(super) fn items(rows: &[(&str, &str)]) -> Vec<Item> {
    rows.iter()
        .map(|(parent, name)| {
            Item::from_entry(
                &Location::new(*parent).unwrap(),
                &FsEntry {
                    name: ProviderPath::from_text(name),
                    kind: EntryKind::File,
                    size: 0,
                    modified_ms: None,
                    attributes: Attributes::default(),
                },
            )
        })
        .collect()
}

pub(super) fn preview(form: &Form, items: &[Item], fold: Fold) -> Option<Preview> {
    let existing = Existing::new(
        items
            .iter()
            .map(|item| ExistingName {
                parent: item.parent.clone(),
                name: item.name.clone(),
            })
            .collect(),
    );
    Preview::compute_form(
        form,
        items,
        Listing {
            existing: &existing,
            fold,
        },
        7,
    )
}

pub(super) fn names(preview: &Preview) -> Vec<&str> {
    preview.rows.iter().map(|row| row.new.as_str()).collect()
}

#[test]
fn edited_names_accept_crlf_and_one_terminal_newline() {
    let mut form = Form::new();
    for text in ["first\nsecond", "first\nsecond\n", "first\r\nsecond\r\n"] {
        form.set_edited_names(text);
        assert_eq!(form.edited_names().unwrap(), ["first", "second"]);
        assert_eq!(form.names_error(2), None);
    }
}

#[test]
fn edited_names_preserve_whitespace_and_interior_empty_lines() {
    let mut form = Form::new();
    form.set_edited_names(" first \r\n\r\n  \nlast\r");
    assert_eq!(
        form.edited_names().unwrap(),
        [" first ", "", "  ", "last\r"]
    );
    form.set_edited_names("first\n\n");
    assert_eq!(form.edited_names().unwrap(), ["first", ""]);
}

#[test]
fn names_count_mismatch_blocks_the_preview() {
    let mut form = Form::new();
    form.set_edited_names("first\nsecond\nthird");
    assert_eq!(
        form.names_error(2),
        Some(NameCount {
            expected: 2,
            actual: 3
        })
    );
    let items = items(&[("/one", "a"), ("/one", "b")]);
    assert!(preview(&form, &items, Fold::Exact).is_none());
    form.set_edited_names("first");
    assert_eq!(
        form.names_error(2),
        Some(NameCount {
            expected: 2,
            actual: 1
        })
    );
    assert!(preview(&form, &items, Fold::Exact).is_none());
}

#[test]
fn copying_a_filename_with_a_newline_reports_a_count_mismatch() {
    let mut form = Form::new();
    let items = items(&[("/one", "line\nbreak.txt")]);
    form.set_edited_names(&items[0].name);
    assert_eq!(
        form.names_error(1),
        Some(NameCount {
            expected: 1,
            actual: 2
        })
    );
    assert!(preview(&form, &items, Fold::Exact).is_none());
}

#[test]
fn explicit_names_override_masks_find_case_and_extension_rules() {
    let mut form = Form::new();
    form.set_name_mask("[C]");
    form.set_ext_mask("bak");
    form.rules.name_case = NameCase::Upper;
    form.set_find(Find {
        text: "custom".into(),
        replace: "changed".into(),
        ..Find::default()
    });
    form.set_edited_names("custom.Jpg\n second.txt");
    let items = items(&[("/one", "a.txt"), ("/one", "b.txt")]);
    let result = preview(&form, &items, Fold::Exact).unwrap();
    assert_eq!(names(&result), ["custom.Jpg", " second.txt"]);
    assert_eq!(result.generation, 7);
    assert!(result.can_run());
}

#[test]
fn invalid_and_empty_edited_names_block_the_plan() {
    let mut form = Form::new();
    form.set_edited_names("bad/name\n\n  \nlast\r");
    let items = items(&[("/one", "a"), ("/one", "b"), ("/one", "c"), ("/one", "d")]);
    let result = preview(&form, &items, Fold::Exact).unwrap();
    assert!(matches!(
        result.rows[0].state,
        RowState::Problem(Problem::Invalid(_))
    ));
    assert_eq!(result.rows[1].state, RowState::Problem(Problem::Empty));
    assert_eq!(result.rows[2].state, RowState::Problem(Problem::Empty));
    assert!(matches!(
        result.rows[3].state,
        RowState::Problem(Problem::Invalid(_))
    ));
    assert!(result.plan.is_none());
}

#[test]
fn edited_duplicates_respect_case_folding_and_folder_boundaries() {
    let mut form = Form::new();
    form.set_edited_names("same\nSAME\nsame");
    let items = items(&[("/one", "a"), ("/one", "b"), ("/two", "c")]);
    let result = preview(&form, &items, Fold::Insensitive).unwrap();
    assert_eq!(
        result.rows[0].state,
        RowState::Problem(Problem::Duplicate(1))
    );
    assert_eq!(
        result.rows[1].state,
        RowState::Problem(Problem::Duplicate(0))
    );
    assert_eq!(result.rows[2].state, RowState::Changes { case_only: false });
    assert!(!result.can_run());
}

#[test]
fn excluded_overrides_keep_the_original_name_occupied() {
    let mut form = Form::new();
    form.set_edited_names("b\nbad/name");
    form.toggle_excluded(1);
    let items = items(&[("/one", "a"), ("/one", "b")]);
    let result = preview(&form, &items, Fold::Exact).unwrap();
    assert_eq!(names(&result), ["b", "b"]);
    assert_eq!(result.rows[0].state, RowState::Problem(Problem::Exists));
    assert_eq!(result.rows[1].state, RowState::Excluded);
    assert!(!result.can_run());
}

#[test]
fn clearing_and_resetting_names_restore_rules_and_preserve_exclusions() {
    let mut form = Form::new();
    assert!(!form.counter_per_folder);
    assert!(!form.has_edited_names());
    form.set_name_mask("[C]");
    form.set_edited_names("custom");
    assert!(form.has_edited_names());
    form.clear_edited_names();
    assert_eq!(form.edited_names(), None);
    assert_eq!(form.names_error(1), None);
    let items = items(&[("/one", "a")]);
    assert_eq!(names(&preview(&form, &items, Fold::Exact).unwrap()), ["1"]);
    form.set_edited_names("custom");
    form.counter_per_folder = true;
    form.toggle_excluded(0);
    form.reset();
    assert_eq!(form.rules, RenameRules::default());
    assert!(!form.counter_per_folder);
    assert!(!form.has_edited_names());
    assert!(form.is_excluded(0));
}

#[test]
fn invalid_rules_still_block_explicit_names() {
    let mut form = Form::new();
    form.set_name_mask("[Q]");
    form.set_edited_names("valid.txt");
    assert!(preview(&form, &items(&[("/one", "a.txt")]), Fold::Exact).is_none());
}

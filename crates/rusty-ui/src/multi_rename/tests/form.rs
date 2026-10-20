use std::time::Instant;

use rusty_core::rename::check::{Existing, ExistingName, Fold, Problem, RowState};
use rusty_core::rename::{ExtCase, Find, NameCase, RenameRules};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::multi_rename::{Form, Item, Listing, Preview};

const CHANGES: RowState = RowState::Changes { case_only: false };

fn exact(existing: &Existing) -> Listing<'_> {
    Listing {
        existing,
        fold: Fold::Exact,
    }
}

fn folder() -> Location {
    Location::new("/work/projects/photos").unwrap()
}

fn entry(name: &str, modified_ms: Option<i64>) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 0,
        modified_ms,
        attributes: Attributes::default(),
    }
}

fn items(names: &[&str]) -> Vec<Item> {
    names
        .iter()
        .map(|name| Item::from_entry(&folder(), &entry(name, None)))
        .collect()
}

fn existing(names: &[&str]) -> Existing {
    Existing::new(
        names
            .iter()
            .map(|name| ExistingName {
                parent: folder(),
                name: (*name).to_owned(),
            })
            .collect(),
    )
}

fn preview(form: &Form, names: &[&str], listing: &[&str]) -> Preview {
    let mut items = items(names);
    form.mark_excluded(&mut items);
    let compiled = form.compile().unwrap();
    Preview::compute(&compiled, &items, exact(&existing(listing)), 1)
}

fn new_names(preview: &Preview) -> Vec<&str> {
    preview.rows.iter().map(|row| row.new.as_str()).collect()
}

#[test]
fn the_defaults_leave_every_row_unchanged() {
    let form = Form::new();
    let names = ["a.txt", "b.jpg", "c"];
    let preview = preview(&form, &names, &names);
    assert!(
        preview
            .rows
            .iter()
            .all(|row| row.state == RowState::Unchanged)
    );
    assert_eq!(preview.summary.unchanged, 3);
    assert_eq!(preview.summary.rename, 0);
    assert!(preview.can_run());
    assert_eq!(form.rules().name_mask, "[N]");
    assert_eq!(form.rules().ext_mask, "[E]");
    assert_eq!(form.rules().find, Find::default());
    assert_eq!(form.rules().name_case, NameCase::Unchanged);
}

#[test]
fn a_mask_change_renames() {
    let mut form = Form::new();
    form.set_name_mask("trip-[C:2]");
    let names = ["a.txt", "b.txt"];
    let preview = preview(&form, &names, &names);
    assert_eq!(new_names(&preview), ["trip-01.txt", "trip-02.txt"]);
    assert_eq!(preview.rows[0].state, CHANGES);
    assert_eq!(preview.summary.rename, 2);
    assert!(preview.plan.is_some());
}

#[test]
fn the_cases_apply_after_the_mask() {
    let mut form = Form::new();
    form.set_cases(NameCase::Upper, ExtCase::Lower);
    let preview = preview(&form, &["abc.TXT"], &["abc.TXT"]);
    assert_eq!(new_names(&preview), ["ABC.txt"]);
}

#[test]
fn parents_and_dates_reach_the_mask() {
    let mut form = Form::new();
    form.set_name_mask("[G]-[P]-[N]");
    let preview = preview(&form, &["a.txt"], &["a.txt"]);
    assert_eq!(new_names(&preview), ["projects-photos-a.txt"]);
    let mut dated = items(&["a.txt"]);
    dated[0] = Item::from_entry(&folder(), &entry("a.txt", Some(1_700_000_000_000)));
    form.set_name_mask("[Y]");
    let compiled = form.compile().unwrap();
    let preview = Preview::compute(&compiled, &dated, exact(&existing(&[])), 1);
    assert_eq!(preview.rows[0].new, "2023.txt");
}

#[test]
fn an_excluded_row_takes_no_number() {
    let mut form = Form::new();
    form.set_name_mask("n[C]");
    form.toggle_excluded(1);
    let names = ["a.txt", "b.txt", "c.txt"];
    let preview = preview(&form, &names, &names);
    assert_eq!(new_names(&preview), ["n1.txt", "b.txt", "n2.txt"]);
    assert_eq!(preview.rows[1].state, RowState::Excluded);
    assert_eq!(preview.summary.excluded, 1);
    form.toggle_excluded(1);
    assert!(!form.is_excluded(1));
}

#[test]
fn a_duplicate_is_a_problem_and_blocks_the_plan() {
    let mut form = Form::new();
    form.set_name_mask("same");
    let names = ["a.txt", "b.txt"];
    let preview = preview(&form, &names, &names);
    assert_eq!(
        preview.rows[0].state,
        RowState::Problem(Problem::Duplicate(1))
    );
    assert_eq!(preview.summary.problems, 2);
    assert!(preview.plan.is_none());
    assert!(!preview.can_run());
    assert_eq!(preview.duplicate_of(0), Some(1));
}

#[test]
fn a_name_in_the_listing_is_a_problem() {
    let mut form = Form::new();
    form.set_name_mask("kept");
    let preview = preview(&form, &["a.txt"], &["a.txt", "kept.txt"]);
    assert_eq!(preview.rows[0].state, RowState::Problem(Problem::Exists));
}

fn colon_form() -> Form {
    let mut form = Form::new();
    form.set_find(Find {
        text: "x".into(),
        replace: ":".into(),
        ..Find::default()
    });
    form
}

#[test]
fn the_problems_only_filter_keeps_problem_rows() {
    let names = ["x1", "b", "x2", "d", "x3"];
    let preview = preview(&colon_form(), &names, &names);
    assert_eq!(preview.visible(false), [0, 1, 2, 3, 4]);
    assert_eq!(preview.visible(true), [0, 2, 4]);
}

#[test]
fn problem_navigation_wraps() {
    let names = ["x1", "b", "x2", "d", "x3"];
    let preview = preview(&colon_form(), &names, &names);
    assert_eq!(preview.next_problem(None), Some(0));
    assert_eq!(preview.next_problem(Some(0)), Some(2));
    assert_eq!(preview.next_problem(Some(3)), Some(4));
    assert_eq!(preview.next_problem(Some(4)), Some(0));
    assert_eq!(preview.previous_problem(None), Some(4));
    assert_eq!(preview.previous_problem(Some(4)), Some(2));
    assert_eq!(preview.previous_problem(Some(1)), Some(0));
    assert_eq!(preview.previous_problem(Some(0)), Some(4));
}

#[test]
fn navigation_without_problems_finds_nothing() {
    let form = Form::new();
    let preview = preview(&form, &["a"], &["a"]);
    assert_eq!(preview.next_problem(Some(0)), None);
    assert_eq!(preview.previous_problem(None), None);
    let empty = Preview::compute(&form.compile().unwrap(), &[], exact(&existing(&[])), 1);
    assert_eq!(empty.next_problem(None), None);
}

#[test]
fn a_preset_round_trips() {
    let mut form = Form::new();
    form.set_name_mask("[N]-[C:3]");
    form.set_ext_mask("[E]");
    form.set_find(Find {
        text: "a".into(),
        replace: "b".into(),
        regex: true,
        match_case: true,
        first_only: true,
        name_only: true,
    });
    form.set_cases(NameCase::Title, ExtCase::Upper);
    form.rules.counter.start = 5;
    let rules: RenameRules = form.rules();
    let loaded = Form::from_preset(&rules);
    assert_eq!(loaded.rules(), rules);
    let mut other = Form::new();
    other.load_preset(&rules);
    assert_eq!(other.rules(), rules);
    other.reset();
    assert_eq!(other.rules(), RenameRules::default());
}

#[test]
fn a_bad_mask_does_not_compile() {
    let mut form = Form::new();
    form.set_name_mask("[Q]");
    assert!(form.compile().is_err());
}

#[test]
#[allow(clippy::print_stderr, reason = "the timing is reported on the CI log")]
#[ignore = "release-only timing check"]
fn fifty_thousand_items_compute_quickly() {
    let mut form = Form::new();
    form.set_name_mask("img-[C:5]-[N]");
    let names: Vec<String> = (0..50_000).map(|n| format!("file{n}.jpg")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let items = items(&refs);
    let compiled = form.compile().unwrap();
    let listing = existing(&refs);
    let start = Instant::now();
    let preview = Preview::compute(
        &compiled,
        &items,
        Listing {
            existing: &listing,
            fold: Fold::Insensitive,
        },
        1,
    );
    let elapsed = start.elapsed();
    eprintln!("computed 50000 items in {elapsed:?}");
    assert_eq!(preview.summary.rename, 50_000);
    assert!(elapsed.as_millis() < 300, "took {elapsed:?}");
}

#[test]
fn the_preview_and_items_cross_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<Preview>();
    assert_send::<Item>();
}

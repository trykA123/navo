//! Unit tests for the plans/README.md link guard.

use super::check;

/// Convert short string literals to the owned names the guard takes.
fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn unlinked_plan_file_fails() {
    let report = check(&names(&["a.md"]), &[], &[]);
    assert_eq!(report.unlinked, names(&["a.md"]));
}

#[test]
fn linked_with_dot_slash_and_anchor_passes() {
    let report = check(&names(&["a.md"]), &names(&["./a.md#top"]), &[]);
    assert!(report.unlinked.is_empty(), "{report:?}");
    assert_eq!(report.linked, 1);
}

#[test]
fn missing_link_target_fails() {
    let report = check(&names(&["a.md"]), &names(&["missing.md"]), &[]);
    assert_eq!(report.missing, names(&["missing.md"]));
}

#[test]
fn allow_listed_file_passes() {
    let report = check(&names(&["a.md"]), &[], &names(&["a.md"]));
    assert!(report.unlinked.is_empty(), "{report:?}");
}

#[test]
fn allow_listed_but_linked_file_fails() {
    let report = check(&names(&["a.md"]), &names(&["a.md"]), &names(&["a.md"]));
    assert_eq!(report.stale_allow, names(&["a.md"]));
}

#[test]
fn readme_is_not_required_to_link_itself() {
    let report = check(&names(&["README.md"]), &[], &[]);
    assert!(report.unlinked.is_empty(), "{report:?}");
}

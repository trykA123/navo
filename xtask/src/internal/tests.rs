//! Unit tests for the guard against tracked internal files.

use super::check;

/// Convert short string literals to the owned paths the guard takes.
fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_string()).collect()
}

#[test]
fn internal_paths_are_reported_sorted_and_user_guide_is_allowed() {
    let hits = check(&paths(&["plans/a.md", "docs/user-guide.md", "docs/x.md"]));
    assert_eq!(hits, paths(&["docs/x.md", "plans/a.md"]));
}

#[test]
fn every_internal_folder_is_reported() {
    let hits = check(&paths(&["spikes/s.rs", "testing/r.md", "design/d.png"]));
    assert_eq!(
        hits,
        paths(&["design/d.png", "spikes/s.rs", "testing/r.md"])
    );
}

#[test]
fn product_paths_and_look_alike_names_pass() {
    let tracked = paths(&["README.md", "crates/a/plans/x.rs", "plans.md", "docsx/a.md"]);
    assert!(check(&tracked).is_empty());
}

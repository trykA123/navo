//! Tests of the shared Viewed store: backends share one, marks persist in the profile and a
//! clear empties it.

use sidewise_host::compare::diff::viewed::ViewedMarks as _;
use sidewise_host::compare::diff::viewed_file::FileViewed;
use sidewise_ui::compare_diff::DiffHooks as _;

use crate::compare_tab_tests::TempDir;
use crate::screens::compare_host::hooks_with;
use crate::viewed_store::open_store;

/// Two backends from `hooks_with` read and write the same store.
#[test]
fn two_backends_share_one_viewed_store() {
    let (first, second) = (hooks_with(None), hooks_with(None));
    let key = [41; 64];
    first.viewed().set(&key, true).expect("mark");
    assert!(second.viewed().get(&key).expect("get"), "the other sees it");
}

/// A mark written through the profile's store is read back by a fresh open of its log.
#[test]
fn marks_persist_across_a_fresh_open_of_the_profile_log() {
    let dir = TempDir::new("viewed-persist");
    let key = [42; 64];
    open_store(Some(dir.path())).set(&key, true).expect("mark");
    let profile = navo_compare_profile::paths::open_profile(Some(dir.path())).expect("profile");
    let again = FileViewed::open(&profile.viewed_file()).expect("reopen");
    assert!(again.get(&key).expect("get"), "the mark survived");
}

/// Clearing through the hooks empties the shared store and reports how many marks it held.
#[test]
fn clearing_through_the_hooks_empties_the_store() {
    let hooks = hooks_with(None);
    let key = [43; 64];
    hooks.viewed().set(&key, true).expect("mark");
    let cleared = hooks.clear_viewed().expect("clear");
    assert!(cleared >= 1, "it counted the mark");
    assert!(!hooks_with(None).viewed().get(&key).expect("get"));
}

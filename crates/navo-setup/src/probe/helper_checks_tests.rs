// Windows tests for the helper's own checks; unelevated, in temp folders on the system drive.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::UI::Shell::FOLDERID_Profile;

use super::helper_chain;
use crate::elevate::{HelperArgs, HelperExit, valid_user_sid};
use crate::payload::platform::security;
use crate::placement::FileId;
use crate::probe::chain::{known_folder, resolve_path};

/// A temp folder at the system drive's root: `%TEMP%` sits inside a user profile, which refuses.
fn outside_profiles() -> tempfile::TempDir {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
    tempfile::Builder::new()
        .prefix(".navo-e4-")
        .tempdir_in(format!("{drive}\\"))
        .expect("temp folder at the system drive root")
}

/// The identity of an existing folder, read through its handle.
fn id_of(path: &Path) -> FileId {
    resolve_path(path)
        .expect("chain resolves")
        .existing
        .last()
        .expect("path exists")
        .id
}

/// Helper arguments for `root` under `parent`, for this process's own user.
fn args(root: PathBuf, parent: FileId) -> HelperArgs {
    HelperArgs {
        root,
        user_sid: security::user_sid().expect("token user SID"),
        parent,
    }
}

#[test]
fn a_fresh_chain_under_its_parent_is_ok() {
    let temp = outside_profiles();
    let parent = id_of(temp.path());
    let chain = helper_chain(&args(temp.path().join("Tools").join("Navo"), parent))
        .expect("a fresh chain outside the refused folders passes");
    assert_eq!(chain.missing, ["Tools", "Navo"]);
    assert_eq!(chain.existing.last().map(|link| link.id), Some(parent));
}

#[test]
fn forward_and_doubled_separators_name_the_same_chain() {
    let temp = outside_profiles();
    let parent = id_of(temp.path());
    let text = format!("{}//Tools/Navo", temp.path().display());
    let chain = helper_chain(&args(PathBuf::from(text), parent))
        .expect("the ids, not the separators, decide");
    assert_eq!(chain.missing, ["Tools", "Navo"]);
}

#[test]
fn a_parent_id_that_differs_is_changed() {
    let temp = outside_profiles();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).expect("sibling folder");
    let result = helper_chain(&args(temp.path().join("Navo"), id_of(&other)));
    assert_eq!(result, Err(HelperExit::Changed));
}

#[test]
fn a_junction_in_the_chain_is_refused() {
    let temp = outside_profiles();
    let real = temp.path().join("real");
    let link = temp.path().join("link");
    std::fs::create_dir(&real).expect("junction target");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&real)
        .status()
        .expect("cmd runs");
    assert!(status.success(), "mklink /J plants the junction unelevated");
    // The parent id is the junction's own, so only the reparse rule can refuse.
    let result = helper_chain(&args(link.join("Navo"), id_of(&link)));
    assert_eq!(result, Err(HelperExit::Refused));
}

#[test]
fn an_existing_root_is_exists() {
    let temp = outside_profiles();
    let root = temp.path().join("Navo");
    std::fs::create_dir(&root).expect("existing root");
    let result = helper_chain(&args(root, id_of(temp.path())));
    assert_eq!(result, Err(HelperExit::Exists));
}

#[test]
fn a_well_formed_sid_of_another_account_is_refused_with_exit_22() {
    let temp = outside_profiles();
    let group = "S-1-5-21-1-2-3-1105";
    assert!(
        valid_user_sid(group),
        "the shape check alone lets it through"
    );
    let mut helper = args(temp.path().join("Navo"), id_of(temp.path()));
    assert_ne!(helper.user_sid, group, "the test account is not the group");
    helper.user_sid = group.to_owned();
    let result = helper_chain(&helper);
    assert_eq!(result, Err(HelperExit::Refused));
    assert_eq!(HelperExit::Refused.code(), 22);
}

#[test]
fn a_root_inside_a_user_profile_is_refused_whatever_its_separators() {
    let profile = known_folder(&FOLDERID_Profile)
        .expect("profile lookup")
        .expect("this account has a profile");
    let text = format!("{}//.navo-e4-missing/Navo", profile.display());
    let result = helper_chain(&args(PathBuf::from(text), id_of(&profile)));
    assert_eq!(result, Err(HelperExit::Refused));
}

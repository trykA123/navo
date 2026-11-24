// Windows tests for the helper's own checks; unelevated, in temp folders on the system drive.

#![cfg(windows)]

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY};
use windows_sys::Win32::UI::Shell::{FOLDERID_Profile, FOLDERID_Windows};

use super::{checked_chain, helper_chain};
use crate::elevate::{HelperArgs, HelperExit, valid_user_sid};
use crate::payload::platform::security;
use crate::placement::{DriveKind, FileId};
use crate::probe::chain::HeldChain;
use crate::probe::chain::tests::junction;
use crate::probe::chain::{known_folder, resolve_path};
use crate::probe::facts::drive_kind;

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
    let held = helper_chain(&args(temp.path().join("Tools").join("Navo"), parent))
        .expect("a fresh chain outside the refused folders passes");
    let chain = &held.chain;
    assert_eq!(chain.missing, ["Tools", "Navo"]);
    assert_eq!(chain.existing.last().map(|link| link.id), Some(parent));
}

#[test]
fn forward_and_doubled_separators_name_the_same_chain() {
    let temp = outside_profiles();
    let parent = id_of(temp.path());
    let text = format!("{}//Tools/Navo", temp.path().display());
    let held = helper_chain(&args(PathBuf::from(text), parent))
        .expect("the ids, not the separators, decide");
    assert_eq!(held.chain.missing, ["Tools", "Navo"]);
}

#[test]
fn a_parent_id_that_differs_is_changed() {
    let temp = outside_profiles();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).expect("sibling folder");
    let result = helper_chain(&args(temp.path().join("Navo"), id_of(&other))).err();
    assert_eq!(result, Some(HelperExit::Changed));
}

#[test]
fn a_junction_in_the_chain_is_refused() {
    let temp = outside_profiles();
    let real = temp.path().join("real");
    let link = temp.path().join("link");
    std::fs::create_dir(&real).expect("junction target");
    junction(&link, &real);
    // The parent id is the junction's own, so only the reparse rule can refuse.
    let result = helper_chain(&args(link.join("Navo"), id_of(&link))).err();
    assert_eq!(result, Some(HelperExit::Refused));
}

#[test]
fn an_existing_root_is_exists() {
    let temp = outside_profiles();
    let root = temp.path().join("Navo");
    std::fs::create_dir(&root).expect("existing root");
    let result = helper_chain(&args(root, id_of(temp.path()))).err();
    assert_eq!(result, Some(HelperExit::Exists));
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
    let result = helper_chain(&helper).err();
    assert_eq!(result, Some(HelperExit::Refused));
    assert_eq!(HelperExit::Refused.code(), 22);
}

#[test]
fn a_root_inside_a_user_profile_is_refused_whatever_its_separators() {
    let profile = known_folder(&FOLDERID_Profile)
        .expect("profile lookup")
        .expect("this account has a profile");
    let text = format!("{}//.navo-e4-missing/Navo", profile.display());
    let result = helper_chain(&args(PathBuf::from(text), id_of(&profile))).err();
    assert_eq!(result, Some(HelperExit::Refused));
}

#[test]
fn an_intermediate_junction_ends_the_walk_and_is_refused() {
    let temp = outside_profiles();
    let real = temp.path().join("real");
    let link = temp.path().join("link");
    std::fs::create_dir_all(real.join("sub")).expect("junction target with a child");
    junction(&link, &real);
    let root = link.join("sub").join("Navo");
    let chain = resolve_path(&root).expect("chain resolves");
    let last = chain.existing.last().expect("an existing folder");
    assert!(
        last.reparse && last.path == link,
        "the walk stops at the junction"
    );
    assert_eq!(chain.missing, ["sub", "Navo"], "nothing behind it resolves");
    // The true parent's id, so a parent check before the reparse rule would say Changed.
    let result = helper_chain(&args(root, id_of(&real.join("sub")))).err();
    assert_eq!(result, Some(HelperExit::Refused));
}

#[test]
fn a_root_inside_the_windows_folder_with_its_true_parent_is_refused() {
    let windows = known_folder(&FOLDERID_Windows)
        .expect("Windows lookup")
        .expect("the Windows folder exists");
    let root = windows.join(".navo-e4-missing").join("Navo");
    // The parent id matches and the drive is local, so only the refused folders can refuse.
    let result = helper_chain(&args(root, id_of(&windows))).err();
    assert_eq!(result, Some(HelperExit::Refused));
}

/// Removes a `subst` drive when the test ends, even when it fails.
struct Subst(String);

impl Drop for Subst {
    fn drop(&mut self) {
        let _ = Command::new("subst").args([self.0.as_str(), "/D"]).status();
    }
}

#[test]
fn a_subst_drive_is_refused() {
    let temp = outside_profiles();
    let letter = ('M'..='Z')
        .rev()
        .find(|letter| !Path::new(&format!("{letter}:\\")).exists())
        .expect("a free drive letter");
    let drive = format!("{letter}:");
    let status = Command::new("subst")
        .arg(&drive)
        .arg(temp.path())
        .status()
        .expect("subst runs");
    assert!(status.success(), "subst maps the drive unelevated");
    let _undo = Subst(drive.clone());
    let root = PathBuf::from(format!("{drive}\\"));
    assert_eq!(
        drive_kind(&root).ok(),
        Some(DriveKind::Fixed),
        "only the subst rule can refuse"
    );
    let result = helper_chain(&args(root.join("Navo"), id_of(&root))).err();
    assert_eq!(result, Some(HelperExit::Refused));
}

/// The helper's exit for a fresh chain on the system drive, judged with a stand-in drive kind.
fn with_drive(drive: fn(&Path) -> std::io::Result<DriveKind>) -> Option<HelperExit> {
    let temp = outside_profiles();
    let held: Result<HeldChain, HelperExit> =
        checked_chain(&args(temp.path().join("Navo"), id_of(temp.path())), drive);
    // The held chain closes before the temp folder is removed.
    let exit = held.err();
    drop(temp);
    exit
}

#[test]
fn a_remote_or_unknown_drive_is_refused_by_the_helper() {
    assert_eq!(
        with_drive(|_| Ok(DriveKind::Fixed)),
        None,
        "control: a local drive passes"
    );
    assert_eq!(
        with_drive(|_| Ok(DriveKind::Remote)),
        Some(HelperExit::Refused)
    );
    assert_eq!(
        with_drive(|_| Ok(DriveKind::Other)),
        Some(HelperExit::Refused)
    );
    let unreadable = with_drive(|_| Err(std::io::Error::other("no drive type")));
    assert_eq!(unreadable, Some(HelperExit::Refused));
}

/// A folder another process holds without sharing is `Busy`, which a retry may pass, not `Refused`.
#[test]
fn a_folder_held_without_sharing_is_busy() {
    let temp = outside_profiles();
    let parent = id_of(temp.path());
    let blocker = OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(temp.path())
        .expect("an exclusive hold on the parent");
    let exit = helper_chain(&args(temp.path().join("Navo"), parent)).err();
    drop(blocker);
    assert_eq!(exit, Some(HelperExit::Busy));
}

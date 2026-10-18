use std::path::PathBuf;

use rusty_store::StoreDirs;

use super::{Profile, choose};
use crate::cli::Start;

fn never_detect() -> Option<StoreDirs> {
    panic!("metrics mode must not look up the user's folders")
}

fn metrics() -> Start {
    Start {
        metrics: Some(PathBuf::from("m.json")),
        ..Start::default()
    }
}

#[test]
fn metrics_without_a_profile_never_resolves_the_user_folders() {
    let profile = choose(&metrics(), never_detect).unwrap();
    assert!(matches!(profile, Profile::Temporary { .. }));
}

#[test]
fn the_temporary_profile_lives_in_the_temp_folder_and_is_deleted_on_exit() {
    let profile = choose(&metrics(), never_detect).unwrap();
    let dirs = profile.dirs().cloned().unwrap();
    let temp = std::env::temp_dir();
    assert!(dirs.config.starts_with(&temp) && dirs.data.starts_with(&temp));
    dirs.ensure().unwrap();
    std::fs::write(dirs.settings_file(), "").unwrap();
    drop(profile);
    assert!(!dirs.config.exists() && !dirs.data.exists());
}

#[test]
fn each_metrics_run_gets_its_own_folder() {
    let first = choose(&metrics(), never_detect).unwrap();
    let second = choose(&metrics(), never_detect).unwrap();
    assert_ne!(first.dirs(), second.dirs());
}

#[test]
fn a_given_profile_is_used_and_kept() {
    let root = tempfile::tempdir().unwrap();
    let start = Start {
        profile: Some(root.path().to_path_buf()),
        ..metrics()
    };
    let profile = choose(&start, never_detect).unwrap();
    let dirs = profile.dirs().cloned().unwrap();
    assert_eq!(dirs, StoreDirs::in_profile(root.path()));
    dirs.ensure().unwrap();
    drop(profile);
    assert!(dirs.config.is_dir() && dirs.data.is_dir());
}

#[test]
fn a_profile_without_metrics_is_used_too() {
    let start = Start {
        profile: Some(PathBuf::from("portable")),
        ..Start::default()
    };
    let profile = choose(&start, never_detect).unwrap();
    assert_eq!(
        profile.dirs(),
        Some(&StoreDirs::in_profile(&PathBuf::from("portable")))
    );
}

#[test]
fn without_flags_the_user_folders_are_used() {
    let user = StoreDirs::in_profile(&PathBuf::from("user"));
    let expected = user.clone();
    let profile = choose(&Start::default(), move || Some(user)).unwrap();
    assert_eq!(profile.dirs(), Some(&expected));
}

#[test]
fn explicit_cleanup_removes_the_profile_without_dropping_it() {
    let profile = choose(&metrics(), never_detect).unwrap();
    let dirs = profile.dirs().cloned().unwrap();
    dirs.ensure().unwrap();
    std::fs::write(dirs.settings_file(), "").unwrap();
    profile.cleanup().run().unwrap();
    assert!(!dirs.config.exists() && !dirs.data.exists());
    profile.cleanup().run().unwrap();
}

#[test]
fn explicit_cleanup_keeps_a_given_profile() {
    let root = tempfile::tempdir().unwrap();
    let start = Start {
        profile: Some(root.path().to_owned()),
        ..metrics()
    };
    let profile = choose(&start, never_detect).unwrap();
    let dirs = profile.dirs().cloned().unwrap();
    dirs.ensure().unwrap();
    profile.cleanup().run().unwrap();
    assert!(dirs.config.exists() && dirs.data.exists());
}

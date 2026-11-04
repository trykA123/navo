use std::fs;
use std::io::Write;
use std::path::Path;

use super::act::{Stage, seam};
use super::test_support::{link, mkdir, tree, try_rename, write};
use super::{GuardError, OutputFolder, PathRole, forbidden_roots, placement, resolve};

const ROLE: PathRole = PathRole::OutputFolder;

#[test]
fn medium_new_output_folder_takes_new_reports_only() {
    let parent = tempfile::tempdir().unwrap();
    let out = OutputFolder::create(&parent.path().join("out"), None).unwrap();
    let mut file = out.create_report(Path::new("report.json")).unwrap();
    file.write_all(b"{}").unwrap();
    drop(file);
    assert_eq!(
        out.create_report(Path::new("report.json")).unwrap_err(),
        GuardError::Exists(PathRole::ScratchRoot)
    );
    assert_eq!(
        fs::read(parent.path().join("out/report.json")).unwrap(),
        b"{}"
    );
}

#[test]
fn medium_report_names_must_be_one_plain_name() {
    let parent = tempfile::tempdir().unwrap();
    let out = OutputFolder::create(&parent.path().join("out"), None).unwrap();
    mkdir(&parent.path().join("out/sub"));
    for name in [
        "../escape",
        "sub/report.json",
        "./report.json",
        "",
        "/report.json",
    ] {
        assert_eq!(
            out.create_report(Path::new(name)).unwrap_err(),
            GuardError::InvalidPath(ROLE),
            "{name}"
        );
    }
    assert!(!parent.path().join("escape").exists());
}

#[test]
fn medium_an_existing_folder_is_refused_and_left_as_it_was() {
    let parent = tempfile::tempdir().unwrap();
    mkdir(&parent.path().join("out"));
    write(&parent.path().join("out/report.json"), b"old");
    let before = tree(parent.path());
    let error = OutputFolder::create(&parent.path().join("out"), None).unwrap_err();
    assert_eq!(error, GuardError::Exists(ROLE));
    assert_eq!(tree(parent.path()), before);
}

#[test]
fn medium_output_inside_the_owner_folder_is_refused_before_creation() {
    let owner = tempfile::tempdir().unwrap();
    let error = OutputFolder::create(&owner.path().join("out"), Some(owner.path())).unwrap_err();
    assert_eq!(error, GuardError::OwnerOverlap(PathRole::OwnerFolder));
    assert_eq!(fs::read_dir(owner.path()).unwrap().count(), 0);
}

#[test]
fn medium_drive_root_and_forbidden_parents_are_refused() {
    let temporary = tempfile::tempdir().unwrap();
    let drive = temporary.path().ancestors().last().unwrap();
    let error = OutputFolder::create(&drive.join("navo-bench-out-x"), None).unwrap_err();
    assert_eq!(error, GuardError::ForbiddenParent(PathRole::ScratchRoot));
    let parent = tempfile::tempdir().unwrap();
    let forbidden = forbidden_roots(Some(&[parent.path().to_path_buf()])).unwrap();
    let result = placement::create_test_output(&parent.path().join("out"), None, &forbidden);
    assert_eq!(
        result.unwrap_err(),
        GuardError::ForbiddenParent(PathRole::ScratchRoot)
    );
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn medium_relative_paths_and_linked_parents_are_refused() {
    assert_eq!(
        OutputFolder::create(Path::new("out"), None).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
    let parent = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    link(elsewhere.path(), &parent.path().join("linked"));
    let error = OutputFolder::create(&parent.path().join("linked/out"), None).unwrap_err();
    assert_eq!(error, GuardError::ReparsePoint(ROLE));
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}

/// During the handoff after the create, `out` moves to `moved` and an impostor folder takes its
/// name. Where the move succeeds (Unix) the result is Left and nothing is deleted; where the held
/// create handle blocks it (Windows) the folder is made and stays at `out`.
#[test]
fn medium_an_output_folder_moved_during_the_handoff_is_never_deleted_by_name() {
    let parent = tempfile::tempdir().unwrap();
    let (out, moved) = (parent.path().join("out"), parent.path().join("moved"));
    let renamed = std::rc::Rc::new(std::cell::Cell::new(false));
    let (from, to, flag) = (out.clone(), moved.clone(), renamed.clone());
    seam::set(Stage::OutputHandoff, move || {
        if try_rename(&from, &to) {
            flag.set(true);
            mkdir(&from);
            write(&from.join("impostor"), b"keep");
        }
    });
    let result = OutputFolder::create(&out, None);
    if renamed.get() {
        assert_eq!(result.unwrap_err(), GuardError::Left(ROLE));
        assert_eq!(
            tree(parent.path()),
            vec![
                (moved, Vec::new()),
                (out.clone(), Vec::new()),
                (out.join("impostor"), b"keep".to_vec())
            ]
        );
    } else {
        let folder = result.unwrap();
        assert!(resolve::equal(
            folder.path(),
            &fs::canonicalize(&out).unwrap()
        ));
        assert!(out.is_dir() && !moved.exists());
    }
}

#[test]
fn medium_an_output_folder_that_fails_its_check_in_place_is_removed() {
    let parent = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let out = parent.path().join("out");
    let (from, to) = (parent.path().to_path_buf(), elsewhere.path().join("p"));
    seam::set(Stage::OutputHandoff, move || {
        try_rename(&from, &to);
    });
    let result = OutputFolder::create(&out, None);
    if elsewhere.path().join("p").exists() {
        assert_eq!(result.unwrap_err(), GuardError::Changed(ROLE));
        assert_eq!(fs::read_dir(elsewhere.path().join("p")).unwrap().count(), 0);
    } else {
        assert!(result.unwrap().path().is_dir());
    }
}

use std::fs;
use std::io::Write;
use std::path::Path;

use super::act::{Stage, seam};
use super::test_support::{append, link, mkdir, rename, tree, write};
use super::{GuardError, MARKER_FILE, PathRole, RecycleError, ScratchRoot, WRITE_API};

const ROLE: PathRole = PathRole::ScratchRoot;

fn root() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, scratch)
}

#[test]
fn small_write_api_is_the_fixed_list_and_matches_the_code() {
    assert_eq!(
        WRITE_API,
        [
            "create_new",
            "create_dir",
            "copy_in",
            "recycle",
            "remove_root",
            "clean_stale_runs",
            "create_report"
        ]
    );
    let source = include_str!("write.rs");
    let public: Vec<&str> = source
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("pub fn "))
        .filter_map(|rest| rest.split('(').next())
        .collect();
    assert_eq!(public, WRITE_API, "every public verb must be listed");
}

#[test]
fn medium_every_verb_refuses_escaping_names() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    let source = outside.path().join("source");
    write(&source, b"bytes");
    let names = [
        Path::new("../escape"),
        Path::new("a/../b"),
        Path::new("./a"),
        Path::new(""),
        Path::new(MARKER_FILE),
        outside.path(),
    ];
    for name in names {
        assert_eq!(
            scratch.create_new(name).unwrap_err(),
            GuardError::InvalidPath(ROLE)
        );
        assert_eq!(
            scratch.create_dir(name).unwrap_err(),
            GuardError::InvalidPath(ROLE)
        );
        assert_eq!(
            scratch.copy_in(&source, name).unwrap_err(),
            GuardError::InvalidPath(ROLE)
        );
    }
    assert_eq!(tree(outside.path()).len(), 1);
}

#[test]
fn medium_every_verb_refuses_a_link_out() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    write(&source.path().join("file"), b"bytes");
    link(outside.path(), &scratch.path().join("out"));
    let before = tree(outside.path());
    for name in ["out", "out/new"] {
        assert!(scratch.create_new(Path::new(name)).is_err());
        assert!(scratch.create_dir(Path::new(name)).is_err());
        assert!(
            scratch
                .copy_in(&source.path().join("file"), Path::new(name))
                .is_err()
        );
    }
    assert_eq!(
        scratch.create_new(Path::new("out/new")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
    assert_eq!(tree(outside.path()), before);
}

#[test]
fn large_a_parent_swapped_for_a_link_between_check_and_create_is_refused() {
    let source = tempfile::tempdir().unwrap();
    write(&source.path().join("file"), b"bytes");
    for verb in ["create_new", "create_dir", "copy_in"] {
        let (_parent, scratch) = root();
        let owner = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&owner.path().join("kept"), b"owner bytes");
        mkdir(&scratch.path().join("sub"));
        let (from, moved, target) = (
            scratch.path().join("sub"),
            elsewhere.path().join("sub"),
            owner.path().to_path_buf(),
        );
        seam::set(Stage::BeforeCreate, move || {
            rename(&from, &moved);
            link(&target, &from);
        });
        let before = tree(owner.path());
        let rel = Path::new("sub/new");
        let error = match verb {
            "create_new" => scratch.create_new(rel).map(drop).unwrap_err(),
            "create_dir" => scratch.create_dir(rel).unwrap_err(),
            _ => scratch
                .copy_in(&source.path().join("file"), rel)
                .unwrap_err(),
        };
        assert_eq!(error, GuardError::ReparsePoint(ROLE), "{verb}");
        assert_eq!(tree(owner.path()), before, "{verb}");
        assert_eq!(
            tree(elsewhere.path()),
            vec![(elsewhere.path().join("sub"), Vec::new())]
        );
    }
}

#[cfg(unix)]
#[test]
fn medium_create_new_refuses_a_dangling_link_to_outside() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("would-be-created");
    link(&target, &scratch.path().join("dangling"));
    assert!(scratch.create_new(Path::new("dangling")).is_err());
    assert!(!target.exists());
}

#[test]
fn medium_create_new_never_overwrites() {
    let (_parent, scratch) = root();
    let mut file = scratch.create_new(Path::new("kept")).unwrap();
    file.write_all(b"original").unwrap();
    drop(file);
    assert_eq!(
        scratch.create_new(Path::new("kept")).unwrap_err(),
        GuardError::Exists(ROLE)
    );
    assert_eq!(fs::read(scratch.path().join("kept")).unwrap(), b"original");
}

#[test]
fn medium_create_dir_needs_an_existing_parent_and_never_reuses() {
    let (_parent, scratch) = root();
    scratch.create_dir(Path::new("made")).unwrap();
    assert!(scratch.path().join("made").is_dir());
    assert_eq!(
        scratch.create_dir(Path::new("made")).unwrap_err(),
        GuardError::Exists(ROLE)
    );
    assert_eq!(
        scratch.create_dir(Path::new("missing/child")).unwrap_err(),
        GuardError::Io(ROLE)
    );
    assert!(!scratch.path().join("missing").exists());
    scratch.create_dir(Path::new("made/child")).unwrap();
    assert!(scratch.path().join("made/child").is_dir());
}

#[test]
fn medium_copy_in_streams_and_returns_the_byte_count() {
    let (_parent, scratch) = root();
    let source = tempfile::tempdir().unwrap();
    let bytes: Vec<u8> = (0..700_000_u32).map(|n| n.to_le_bytes()[0]).collect();
    write(&source.path().join("big"), &bytes);
    let count = scratch
        .copy_in(&source.path().join("big"), Path::new("copy"))
        .unwrap();
    assert_eq!(count, 700_000);
    assert_eq!(fs::read(scratch.path().join("copy")).unwrap(), bytes);
    assert_eq!(fs::read(source.path().join("big")).unwrap(), bytes);
}

#[test]
fn medium_copy_in_never_overwrites_an_existing_destination() {
    let (_parent, scratch) = root();
    let source = tempfile::tempdir().unwrap();
    write(&source.path().join("file"), b"new");
    drop(scratch.create_new(Path::new("dest")).unwrap());
    append(&scratch.path().join("dest"), b"old");
    assert_eq!(
        scratch
            .copy_in(&source.path().join("file"), Path::new("dest"))
            .unwrap_err(),
        GuardError::Exists(ROLE)
    );
    assert_eq!(fs::read(scratch.path().join("dest")).unwrap(), b"old");
}

#[test]
fn medium_copy_in_hash_mismatch_removes_the_destination() {
    let (_parent, scratch) = root();
    let source = tempfile::tempdir().unwrap();
    write(&source.path().join("file"), b"source bytes");
    let dest = scratch.path().join("dest");
    let corrupt = dest.clone();
    seam::set(Stage::BeforeHashCheck, move || append(&corrupt, b"!"));
    assert_eq!(
        scratch
            .copy_in(&source.path().join("file"), Path::new("dest"))
            .unwrap_err(),
        GuardError::Mismatch(ROLE)
    );
    assert!(!dest.exists());
    assert_eq!(
        fs::read(source.path().join("file")).unwrap(),
        b"source bytes"
    );
}

#[test]
fn medium_copy_in_bad_source_creates_nothing() {
    let (_parent, scratch) = root();
    let source = tempfile::tempdir().unwrap();
    assert_eq!(
        scratch
            .copy_in(&source.path().join("missing"), Path::new("dest"))
            .unwrap_err(),
        GuardError::Io(PathRole::CopySource)
    );
    // Windows refuses to open a folder as a file, so a folder source fails at the open there.
    let folder_refusal = if cfg!(windows) {
        GuardError::Io(PathRole::CopySource)
    } else {
        GuardError::InvalidPath(PathRole::CopySource)
    };
    assert_eq!(
        scratch
            .copy_in(source.path(), Path::new("dest"))
            .unwrap_err(),
        folder_refusal
    );
    assert!(!scratch.path().join("dest").exists());
}

#[test]
fn small_new_errors_show_roles_and_codes_only() {
    assert_eq!(
        GuardError::Recycle(PathRole::StaleRun, RecycleError::PermanentDelete).to_string(),
        "stale-run: recycle-permanent-delete"
    );
    assert_eq!(
        GuardError::Mismatch(PathRole::ScratchRoot).to_string(),
        "scratch-root: hash-mismatch"
    );
    assert_eq!(
        GuardError::Io(PathRole::CopySource).to_string(),
        "copy-source: io"
    );
    assert_eq!(
        GuardError::Left(PathRole::ScratchRoot).to_string(),
        "scratch-root: stray-left"
    );
    let (_parent, scratch) = root();
    mkdir(&scratch.path().join("private-name"));
    let error = scratch.create_dir(Path::new("private-name")).unwrap_err();
    assert_eq!(error.to_string(), "scratch-root: exists");
}

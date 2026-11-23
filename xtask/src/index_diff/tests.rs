//! Fixture tests for `index-diff`: counted differences, hard links and skip rules.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use navo_index::{
    file::{self, Header},
    table::{Flags, NO_PARENT, NameTable},
    usn::FrnMap,
};

use super::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Create a unique temporary base folder with an empty `tree` inside it.
fn fixture(label: &str) -> (PathBuf, PathBuf) {
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "navo-index-diff-{label}-{}-{number}",
        std::process::id()
    ));
    let tree = base.join("tree");
    fs::create_dir_all(&tree).unwrap();
    (base, tree)
}

/// Save a snapshot whose root is `root` and whose direct children are `entries`.
fn snapshot(base: &Path, root: &Path, entries: &[(&str, bool)]) -> PathBuf {
    let root_label = display_path(root);
    let mut table = NameTable::with_capacity(entries.len() + 1, 1024);
    let root_id = table.push(NO_PARENT, &root_label, Flags::FOLDER).unwrap();
    let mut ids = FrnMap::new();
    ids.insert(0, root_id);
    for (index, (name, folder)) in entries.iter().enumerate() {
        let flags = if *folder { Flags::FOLDER } else { Flags::NONE };
        let id = table.push(root_id, name, flags).unwrap();
        ids.insert(u128::try_from(index + 1).unwrap(), id);
    }
    let header = Header {
        volume_serial: 0,
        journal_id: 0,
        next_usn: 0,
        root: root_label,
        built_at: 0,
    };
    let path = base.join("snapshot.index");
    file::save_with_ids(&path, &header, &table, &ids).unwrap();
    path
}

/// Build `Options` for one fixture.
fn options(index: PathBuf, root: &Path) -> Options {
    Options {
        snapshot: index,
        root: root.to_path_buf(),
        allow_missing: 0,
    }
}

/// Build the CLI arguments for one snapshot and root pair.
fn args(snapshot: &Path, root: &Path) -> Vec<String> {
    vec![
        "--snapshot".to_owned(),
        snapshot.display().to_string(),
        "--root".to_owned(),
        root.display().to_string(),
    ]
}

#[test]
/// One indexed path absent from disk and one disk path absent from the index fail.
fn one_extra_and_one_missing_path_are_counted() {
    let (base, tree) = fixture("red");
    fs::write(tree.join("kept.txt"), b"kept").unwrap();
    fs::write(tree.join("extra.txt"), b"extra").unwrap();
    let index = snapshot(&base, &tree, &[("kept.txt", false), ("missing.txt", false)]);
    let report = diff(&options(index.clone(), &tree)).unwrap();
    assert_eq!((report.missing, report.extra, report.wrong_kind), (1, 1, 0));
    assert_eq!((report.entries, report.walked), (3, 3));
    assert!(report.failed(0));
    assert!(!report.failed(2));
    let args = args(&index, &tree);
    assert_eq!(run(&args), ExitCode::FAILURE);
    let mut allowed = args.to_vec();
    allowed.extend(["--allow-missing".to_owned(), "2".to_owned()]);
    assert_eq!(run(&allowed), ExitCode::SUCCESS);
    fs::remove_dir_all(&base).unwrap();
}

#[test]
/// An unindexed hard link to an indexed file is not reported as extra.
fn hard_links_are_present_when_one_path_is_indexed() {
    let (base, tree) = fixture("links");
    fs::write(tree.join("a.txt"), b"same file").unwrap();
    fs::hard_link(tree.join("a.txt"), tree.join("b.txt")).unwrap();
    let index = snapshot(&base, &tree, &[("a.txt", false)]);
    let report = diff(&options(index, &tree)).unwrap();
    assert_eq!((report.entries, report.walked), (2, 3));
    assert_eq!((report.missing, report.extra, report.wrong_kind), (0, 0, 0));
    fs::remove_dir_all(&base).unwrap();
}

#[test]
/// A path indexed as a file but found as a folder counts as the wrong kind.
fn a_folder_indexed_as_a_file_is_a_kind_mismatch() {
    let (base, tree) = fixture("kind");
    fs::create_dir(tree.join("thing")).unwrap();
    let index = snapshot(&base, &tree, &[("thing", false)]);
    let report = diff(&options(index, &tree)).unwrap();
    assert_eq!((report.missing, report.extra, report.wrong_kind), (0, 0, 1));
    assert!(report.failed(0));
    fs::remove_dir_all(&base).unwrap();
}

#[test]
/// Root metadata entries are skipped in the snapshot and in the walk.
fn dollar_metadata_and_system_volume_information_are_skipped() {
    let (base, tree) = fixture("skip");
    fs::write(tree.join("$MFT"), b"").unwrap();
    let system = tree.join("System Volume Information");
    fs::create_dir(&system).unwrap();
    fs::write(system.join("hidden.txt"), b"").unwrap();
    let index = snapshot(
        &base,
        &tree,
        &[("$MFT", false), ("System Volume Information", true)],
    );
    let report = diff(&options(index, &tree)).unwrap();
    assert_eq!((report.entries, report.walked), (1, 1));
    assert_eq!((report.missing, report.extra, report.wrong_kind), (0, 0, 0));
    fs::remove_dir_all(&base).unwrap();
}

#[cfg(unix)]
#[test]
/// Directory symlinks count as folders, dangling links as files, and neither is followed.
fn symlinks_get_their_kind_without_being_followed() {
    use std::os::unix::fs::symlink;
    let (base, tree) = fixture("symlink");
    fs::create_dir(tree.join("target")).unwrap();
    fs::write(tree.join("target/inside.txt"), b"").unwrap();
    symlink(tree.join("target"), tree.join("dir-link")).unwrap();
    symlink(tree.join("missing-target"), tree.join("dangling")).unwrap();
    let walked = walk(&tree).unwrap();
    let label = display_path(&tree);
    let kinds: std::collections::HashMap<&str, Kind> = walked
        .iter()
        .map(|entry| (entry.path.as_str(), entry.kind))
        .collect();
    assert_eq!(
        kinds.get(format!("{label}\\dir-link").as_str()),
        Some(&Kind::Folder)
    );
    assert_eq!(
        kinds.get(format!("{label}\\dangling").as_str()),
        Some(&Kind::File)
    );
    assert!(
        !kinds.contains_key(format!("{label}\\dir-link\\inside.txt").as_str()),
        "the walk followed a directory symlink"
    );
    assert_eq!(walked.len(), 5);
    fs::remove_dir_all(&base).unwrap();
}

#[test]
/// A missing snapshot fails the command and names the file it could not load.
fn a_missing_snapshot_fails_with_its_path() {
    let (base, tree) = fixture("missing-snapshot");
    let snapshot = base.join("absent.index");
    let error = diff(&options(snapshot.clone(), &tree)).unwrap_err();
    assert!(error.contains("cannot load snapshot"), "{error}");
    assert!(error.contains(&snapshot.display().to_string()), "{error}");
    assert_eq!(run(&args(&snapshot, &tree)), ExitCode::FAILURE);
    fs::remove_dir_all(&base).unwrap();
}

#[test]
/// A corrupt snapshot file fails the command instead of comparing garbage.
fn a_corrupt_snapshot_fails_with_its_path() {
    let (base, tree) = fixture("corrupt-snapshot");
    let snapshot = base.join("corrupt.index");
    fs::write(&snapshot, [0_u8; 70]).unwrap();
    let error = diff(&options(snapshot.clone(), &tree)).unwrap_err();
    assert!(error.contains("cannot load snapshot"), "{error}");
    assert!(error.contains("not a Navo index file"), "{error}");
    assert_eq!(run(&args(&snapshot, &tree)), ExitCode::FAILURE);
    fs::remove_dir_all(&base).unwrap();
}

#[cfg(unix)]
#[test]
/// An unreadable folder fails the run instead of silently dropping its entries.
fn an_unreadable_folder_fails_the_run() {
    use std::os::unix::fs::PermissionsExt as _;
    let (base, tree) = fixture("unreadable");
    let blocked = tree.join("blocked");
    fs::create_dir(&blocked).unwrap();
    let index = snapshot(&base, &tree, &[("blocked", true)]);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&blocked).is_ok() {
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&base).unwrap();
        return;
    }
    let error = walk(&tree).unwrap_err();
    let code = run(&args(&index, &tree));
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
    fs::remove_dir_all(&base).unwrap();
    assert!(error.contains("blocked"), "{error}");
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
/// A lower-case drive letter is rendered upper case so `x:\` matches `X:`.
fn drive_letters_are_upper_cased() {
    assert_eq!(display_path(Path::new("c:\\tree")), "C:\\tree");
    assert_eq!(display_path(Path::new("d:\\tree")), "D:\\tree");
    assert_eq!(display_path(Path::new("X:\\tree")), "X:\\tree");
}

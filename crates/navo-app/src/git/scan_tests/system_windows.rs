use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_fs::DriveClass;

use super::super::watches::Watches;
use super::*;

fn within(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while !done() {
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}

/// A temp folder with `repo/.git` and `repo/sub`, on the system drive.
fn repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("sub")).unwrap();
    (dir, root)
}

fn armed(root: &Path) -> Watches {
    let mut watches = Watches::new(Arc::new(SystemProbe));
    arm_live(&mut watches, root, root);
    watches
}

fn arm_live(watches: &mut Watches, root: &Path, listing: &Path) {
    watches.arm(root, listing, DriveClass::Ssd);
    let live = within(Duration::from_secs(2), || watches.mark(root).is_some());
    assert!(live, "a fixed-drive watch must go live within 2 s");
}

fn changes(watches: &mut Watches, root: &Path) -> u64 {
    watches.mark(root).expect("the watch stays live").changes
}

fn raised_by(watches: &mut Watches, root: &Path, before: u64, by: u64) -> bool {
    within(Duration::from_secs(1), || {
        changes(watches, root) >= before + by
    })
}

fn location(path: &Path) -> Location {
    Location::new(path.to_str().unwrap()).unwrap()
}

fn junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .unwrap();
    assert!(status.success(), "mklink /J failed");
}

#[test]
fn mark_is_some_within_two_seconds_of_arming() {
    let (_dir, root) = repo();
    let mut watches = armed(&root);
    assert!(watches.mark(&root).is_some());
}

#[test]
fn writes_in_a_subfolder_and_to_the_index_raise_changes() {
    let (_dir, root) = repo();
    let mut watches = armed(&root);
    let before = changes(&mut watches, &root);
    fs::write(root.join("sub").join("a.txt"), "a").unwrap();
    assert!(raised_by(&mut watches, &root, before, 1), "subfolder write");
    let before = changes(&mut watches, &root);
    fs::write(root.join(".git").join("index"), "i").unwrap();
    assert!(
        raised_by(&mut watches, &root, before, 1),
        ".git/index write"
    );
}

#[test]
fn create_then_remove_raises_changes_by_two() {
    let (_dir, root) = repo();
    let mut watches = armed(&root);
    let before = changes(&mut watches, &root);
    let file = root.join("brief.txt");
    fs::File::create(&file).unwrap();
    fs::remove_file(&file).unwrap();
    assert!(raised_by(&mut watches, &root, before, 2));
}

#[test]
fn renamed_root_gives_no_mark() {
    let (dir, root) = repo();
    let mut watches = armed(&root);
    fs::rename(&root, dir.path().join("moved")).unwrap();
    assert_eq!(watches.mark(&root), None);
}

#[test]
fn replaced_root_gives_no_mark() {
    let (dir, root) = repo();
    let mut watches = armed(&root);
    fs::rename(&root, dir.path().join("old")).unwrap();
    fs::create_dir(&root).unwrap();
    assert_eq!(watches.mark(&root), None);
}

#[test]
fn junction_between_listing_and_root_gives_unknown() {
    let (dir, root) = repo();
    let link = dir.path().join("link");
    junction(&link, &root);
    let fixed = |_: &Location| DriveClass::Ssd;
    let plain = root_class_with(&root, &location(&root), navo_fs::volume_mount_point, fixed);
    assert_eq!(
        plain,
        DriveClass::Ssd,
        "a plain root keeps the listing's class"
    );
    let from_parent = root_class_with(
        &root,
        &location(dir.path()),
        navo_fs::volume_mount_point,
        fixed,
    );
    assert_eq!(
        from_parent,
        DriveClass::Ssd,
        "a plain parent listing keeps its class"
    );
    let linked = link.join("sub");
    let class = root_class_with(
        &link,
        &location(&linked),
        navo_fs::volume_mount_point,
        fixed,
    );
    assert_eq!(class, DriveClass::Unknown, "the root is a junction");
    let class = root_class_with(
        &link,
        &location(dir.path()),
        navo_fs::volume_mount_point,
        fixed,
    );
    assert_eq!(
        class,
        DriveClass::Unknown,
        "the listing reaches the root through a junction"
    );
}

#[test]
fn root_off_its_drive_letter_volume_gives_unknown() {
    let (dir, root) = repo();
    let fixed = |_: &Location| DriveClass::Ssd;
    let mounted = dir.path().to_path_buf();
    let class = root_class_with(&root, &location(&root), |_| Ok(mounted), fixed);
    assert_eq!(
        class,
        DriveClass::Unknown,
        "a folder mount point is not the drive root"
    );
    let failed = |_: &Path| Err(io::Error::other("no volume"));
    let class = root_class_with(&root, &location(&root), failed, fixed);
    assert_eq!(class, DriveClass::Unknown, "a failed read is unknown");
    let letter = root.ancestors().last().unwrap().to_path_buf();
    let class = root_class_with(&root, &location(&root), |_| Ok(letter), fixed);
    assert_eq!(
        class,
        DriveClass::Ssd,
        "the drive-letter root keeps the class"
    );
}

#[test]
fn armed_watches_leave_folders_outside_the_listing_path_free() {
    let dir = tempfile::tempdir().unwrap();
    let listing = dir.path().join("listing");
    let repo = listing.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    let sibling = dir.path().join("sibling");
    fs::create_dir_all(sibling.join("inner")).unwrap();
    let mut watches = Watches::new(Arc::new(SystemProbe));
    let folder = navo_fs::os_path(&location(&listing)).unwrap();
    arm_live(&mut watches, &repo, &folder);
    arm_live(&mut watches, &listing, &folder);
    let moved = dir.path().join("sibling-moved");
    fs::rename(&sibling, &moved).expect("R3: renaming a folder off the listing path must succeed");
    fs::remove_dir_all(&moved).expect("R3: deleting a folder off the listing path must succeed");
    fs::rename(&repo, listing.join("repo-moved"))
        .expect("R3: renaming the armed direct-child repository must succeed");
    drop(watches);
}

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::{symlink_dir, symlink_file};
use std::process::Command;

use super::windows::attrib;
use super::*;

const LONG_SEGMENT: usize = 90;

fn assert_same_listing(folder: &Location, expected: usize) {
    let (native_result, native) = list_with(&LocalFs::new(), folder);
    let (std_result, std) = list_with(&LocalFs::new().with_std_listing(), folder);
    assert_eq!((native_result, std_result), (Ok(()), Ok(())));
    let (native, std) = (entries(&native), entries(&std));
    assert_eq!(native.len(), expected, "{native:?}");
    assert_eq!(native, std);
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
fn native_listing_matches_read_dir_for_files_folders_and_attributes() {
    let (dir, folder) = temp();
    write_file(dir.path(), "sized.bin", 4097);
    write_file(dir.path(), "empty.txt", 0);
    write_file(dir.path(), "hidden.txt", 1);
    write_file(dir.path(), "system.txt", 1);
    write_file(dir.path(), "readonly.txt", 1);
    fs::create_dir(dir.path().join("folder")).unwrap();
    fs::create_dir(dir.path().join("hidden-folder")).unwrap();
    attrib("+h", &dir.path().join("hidden.txt"));
    attrib("+s", &dir.path().join("system.txt"));
    attrib("+h", &dir.path().join("hidden-folder"));
    let mut permissions = fs::metadata(dir.path().join("readonly.txt"))
        .unwrap()
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(dir.path().join("readonly.txt"), permissions).unwrap();
    assert_same_listing(&folder, 7);
}

#[test]
fn native_listing_matches_read_dir_for_unicode_names() {
    let (dir, folder) = temp();
    for name in ["ünïcødé ✓.txt", "日本語", "🦀.rs", "Ελληνικά"] {
        write_file(dir.path(), name, 2);
    }
    let lone_surrogate = OsString::from_wide(&[0xD800, u16::from(b'x')]);
    fs::write(dir.path().join(lone_surrogate), b"x").unwrap();
    assert_same_listing(&folder, 5);
}

#[test]
fn native_listing_matches_read_dir_past_260_chars() {
    let (dir, folder) = temp();
    let segment = "d".repeat(LONG_SEGMENT);
    let mut deep = dir.path().to_path_buf();
    let mut deep_folder = folder;
    for _ in 0..3 {
        deep.push(&segment);
        deep_folder = deep_folder
            .join(&ProviderPath::from_text(&segment))
            .unwrap();
    }
    fs::create_dir_all(deep.join("inner")).unwrap();
    write_file(&deep, "file.txt", 9);
    assert!(deep.join("file.txt").as_os_str().len() > 260);
    assert_same_listing(&deep_folder, 2);
}

#[test]
fn native_listing_matches_read_dir_for_links() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("target")).unwrap();
    write_file(dir.path(), "file", 3);
    junction(&dir.path().join("junction"), &dir.path().join("target"));
    let folder_link = symlink_dir(dir.path().join("target"), dir.path().join("dir-link"));
    let file_link = symlink_file(dir.path().join("file"), dir.path().join("file-link"));
    let expected = 3 + usize::from(folder_link.is_ok()) + usize::from(file_link.is_ok());
    assert_same_listing(&folder, expected);
}

#[test]
fn a_junction_is_a_link_to_a_folder() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("target")).unwrap();
    junction(&dir.path().join("junction"), &dir.path().join("target"));
    let link = entry_named(&folder, "junction");
    assert_eq!(
        (link.kind, link.size, link.attributes.link_to_folder),
        (EntryKind::Link, 0, true)
    );
}

#[test]
fn a_file_symlink_is_not_a_link_to_a_folder() {
    let (dir, folder) = temp();
    write_file(dir.path(), "target", 7);
    if symlink_file(dir.path().join("target"), dir.path().join("link")).is_err() {
        return;
    }
    let link = entry_named(&folder, "link");
    assert_eq!(
        (link.kind, link.size, link.attributes.link_to_folder),
        (EntryKind::Link, 0, false)
    );
}

// Windows tests for exact-path shortcut removal, using real shell links in temp folders.

#![cfg(windows)]

use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};
use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_StartMenu};
use windows_sys::core::GUID;

use super::files::{Apartment, LINK_LIMIT, wide_z};
use super::*;

/// One temp tree with a root, a sibling and the two known-folder stand-ins.
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    desktop: PathBuf,
    start: PathBuf,
}

/// Builds `<temp>\Navo`, `<temp>\NavoOld`, `<temp>\desktop` and `<temp>\start\Programs`.
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Navo");
    let desktop = temp.path().join("desktop");
    let start = temp.path().join("start");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(temp.path().join("NavoOld")).unwrap();
    fs::create_dir_all(&desktop).unwrap();
    fs::create_dir_all(start.join("Programs")).unwrap();
    Fixture {
        _temp: temp,
        root,
        desktop,
        start,
    }
}

/// Compares two GUIDs field by field; windows-sys GUIDs do not implement `PartialEq`.
fn same_guid(left: &GUID, right: &GUID) -> bool {
    left.data1 == right.data1
        && left.data2 == right.data2
        && left.data3 == right.data3
        && left.data4 == right.data4
}

/// Maps the two known-folder GUIDs to the fixture folders.
fn lookup(fixture: &Fixture) -> impl Fn(&GUID) -> std::io::Result<Option<PathBuf>> {
    let desktop = fixture.desktop.clone();
    let start = fixture.start.clone();
    move |id: &GUID| {
        Ok(if same_guid(id, &FOLDERID_Desktop) {
            Some(desktop.clone())
        } else if same_guid(id, &FOLDERID_StartMenu) {
            Some(start.clone())
        } else {
            None
        })
    }
}

/// Saves one real shell link at `path` pointing to `target`.
fn save_link(path: &Path, target: &Path) {
    let _apartment = Apartment::new().unwrap();
    let target = wide_z(target);
    let saved = wide_z(path);
    // SAFETY: COM is initialized; the wide paths live across these synchronous calls and all
    // interfaces are released before this helper returns.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
        link.SetPath(PCWSTR(target.as_ptr())).unwrap();
        let persist: IPersistFile = link.cast().unwrap();
        persist.Save(PCWSTR(saved.as_ptr()), true).unwrap();
    }
}

/// Desktop link into the root is removed.
#[test]
fn a_link_into_the_root_is_deleted() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(&link, &fixture.root.join("current").join("navo.exe"));
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.removed, vec![link.clone()]);
    assert!(report.kept.is_empty());
    assert!(!link.exists());
}

/// Desktop link to another folder is kept.
#[test]
fn a_link_elsewhere_is_kept() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(
        &link,
        &fixture._temp.path().join("NavoOld").join("navo.exe"),
    );
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.kept, vec![link.clone()]);
    assert!(link.exists());
}

/// A non-shell file named Navo.lnk is kept.
#[test]
fn a_file_named_navo_lnk_that_is_not_a_shell_link_is_kept() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    fs::write(&link, b"not a shell link").unwrap();
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.kept, vec![link.clone()]);
    assert!(link.exists());
}

/// A ticked desktop box leaves its link alone.
#[test]
fn a_ticked_box_leaves_its_link() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(&link, &fixture.root.join("navo.exe"));
    let report = remove_unticked_with(&fixture.root, true, true, lookup(&fixture));
    assert!(report.removed.is_empty());
    assert!(report.kept.is_empty());
    assert!(link.exists());
}

/// A link whose target is the root folder itself counts as inside.
#[test]
fn a_link_to_the_root_folder_itself_counts_as_inside() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(&link, &fixture.root);
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.removed, vec![link.clone()]);
    assert!(!link.exists());
}

/// A sibling whose name starts with the root's name is not inside.
#[test]
fn a_link_to_a_sibling_with_the_same_prefix_is_kept() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(
        &link,
        &fixture._temp.path().join("NavoOld").join("navo.exe"),
    );
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.kept, vec![link.clone()]);
    assert!(link.exists());
}

/// The start-menu box uses only `<StartMenu>\Programs\Navo.lnk`.
#[test]
fn the_start_menu_link_is_deleted_at_its_exact_path() {
    let fixture = fixture();
    let link = fixture.start.join("Programs").join("Navo.lnk");
    save_link(&link, &fixture.root.join("navo.exe"));
    let report = remove_unticked_with(&fixture.root, true, false, lookup(&fixture));
    assert_eq!(report.removed, vec![link.clone()]);
    assert!(!link.exists());
}

/// A real link padded past the read cap is kept, whatever its target.
#[test]
fn an_oversized_link_is_kept() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(&link, &fixture.root.join("navo.exe"));
    let mut bytes = fs::read(&link).unwrap();
    bytes.resize(LINK_LIMIT + 1, 0);
    fs::write(&link, bytes).unwrap();
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.kept, vec![link.clone()]);
    assert!(link.exists());
}

/// A link held open for write is kept: the open shares only read.
#[test]
fn a_link_held_open_for_write_is_kept() {
    let fixture = fixture();
    let link = fixture.desktop.join("Navo.lnk");
    save_link(&link, &fixture.root.join("navo.exe"));
    let held = fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(&link)
        .unwrap();
    let report = remove_unticked_with(&fixture.root, false, true, lookup(&fixture));
    assert_eq!(report.kept, vec![link.clone()]);
    assert!(link.exists());
    drop(held);
}

use std::path::PathBuf;
use std::process::Command;

use super::*;

fn os_path_of(text: &str) -> PathBuf {
    os_path(&Location::new(text).unwrap()).unwrap()
}

fn attrib(flag: &str, path: &Path) {
    let status = Command::new("attrib").arg(flag).arg(path).status().unwrap();
    assert!(status.success(), "attrib {flag} failed");
}

#[test]
fn a_drive_path_gets_the_verbatim_prefix() {
    assert_eq!(os_path_of(r"C:\a"), PathBuf::from(r"\\?\C:\a"));
}

#[test]
fn a_unc_path_gets_the_verbatim_unc_prefix() {
    assert_eq!(
        os_path_of(r"\\srv\share\x"),
        PathBuf::from(r"\\?\UNC\srv\share\x")
    );
}

#[test]
fn a_verbatim_path_is_unchanged() {
    assert_eq!(os_path_of(r"\\?\C:\a"), PathBuf::from(r"\\?\C:\a"));
}

#[test]
fn a_verbatim_unc_path_is_unchanged() {
    assert_eq!(
        os_path_of(r"\\?\UNC\srv\share\x"),
        PathBuf::from(r"\\?\UNC\srv\share\x")
    );
}

#[test]
fn a_bytes_path_is_name_invalid() {
    let location = Location::local(ProviderPath::from_bytes(&[b'C', b':', 0xFF])).unwrap();
    assert_eq!(kind_of(os_path(&location)), Some(FsErrorKind::NameInvalid));
}

#[test]
fn a_hidden_file_is_hidden() {
    let (dir, folder) = temp();
    write_file(dir.path(), "h", 1);
    attrib("+h", &dir.path().join("h"));
    assert!(entry_named(&folder, "h").attributes.hidden);
}

#[test]
fn a_system_file_is_system() {
    let (dir, folder) = temp();
    write_file(dir.path(), "s", 1);
    attrib("+s", &dir.path().join("s"));
    assert!(entry_named(&folder, "s").attributes.system);
}

#[test]
fn a_dot_file_is_not_hidden_on_windows() {
    let (dir, folder) = temp();
    write_file(dir.path(), ".rc", 1);
    assert!(!entry_named(&folder, ".rc").attributes.hidden);
}

#[test]
fn the_drive_root_is_a_folder_named_by_its_whole_path() {
    let root = Location::new(r"C:\").unwrap();
    let entry = LocalFs::new().metadata(&root).unwrap();
    assert_eq!(
        (entry.name, entry.kind),
        (ProviderPath::from_text(r"C:\"), EntryKind::Folder)
    );
}

#[test]
fn recycle_moves_a_file_out_of_its_folder() {
    use crate::ProviderOps;
    let (dir, root) = temp();
    write_file(dir.path(), "recycle-me.txt", 3);
    let location = root
        .join(&ProviderPath::from_text("recycle-me.txt"))
        .unwrap();
    LocalFs::new().recycle(&location).unwrap();
    assert!(!dir.path().join("recycle-me.txt").exists());
}

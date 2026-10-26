use super::*;
use crate::{ProviderOps, RenameOutcome};

#[test]
#[ignore = "requires a mounted Windows exFAT image in FERRITE_EXFAT_ROOT"]
fn native_exfat_renames_without_a_proved_identity() {
    unproved("FERRITE_EXFAT_ROOT", "exfat");
}

#[test]
#[ignore = "requires a mounted Windows FAT32 image in FERRITE_FAT32_ROOT"]
fn native_fat32_renames_without_a_proved_identity() {
    unproved("FERRITE_FAT32_ROOT", "fat32");
}

fn unproved(variable: &str, expected: &str) {
    let root = std::env::var_os(variable).expect("set the variable to the mounted image root");
    let dir = tempfile::tempdir_in(root).unwrap();
    assert_eq!(filesystem(dir.path()).to_ascii_lowercase(), expected);
    let fs = LocalFs::new();
    let root = location_of(dir.path());
    fs::write(dir.path().join("a"), b"first file").unwrap();
    assert_eq!(fs.file_identity(&child("a", &root)).unwrap(), None);
    let before = fs.metadata(&child("a", &root)).unwrap();
    assert_eq!(
        super::rename_batch::rename_all(&root, &[("a", "a-longer-renamed-file")]),
        RenameOutcome::Done
    );
    let after = fs.metadata(&child("a-longer-renamed-file", &root)).unwrap();
    assert_eq!(
        crate::FileIdentity::fingerprint(&after),
        crate::FileIdentity::fingerprint(&before)
    );
}

#[test]
#[ignore = "requires a Windows SMB test share in FERRITE_SMB_ROOT"]
fn native_smb_identity_and_batch_rename() {
    let root = std::env::var_os("FERRITE_SMB_ROOT").expect("set FERRITE_SMB_ROOT to a test share");
    let dir = tempfile::tempdir_in(root).unwrap();
    exercise(dir.path());
}

fn exercise(path: &Path) {
    let fs = LocalFs::new();
    let root = location_of(path);
    let folder = fs
        .file_identity(&root)
        .unwrap()
        .expect("directory identity");
    assert_eq!(fs.file_identity(&root).unwrap(), Some(folder));
    fs::write(path.join("a"), b"first file").unwrap();
    fs::write(path.join("b"), b"second file").unwrap();
    let first = fs
        .file_identity(&child("a", &root))
        .unwrap()
        .expect("file identity");
    let second = fs
        .file_identity(&child("b", &root))
        .unwrap()
        .expect("file identity");
    assert_ne!(first, second);
    assert_ne!(first, folder);
    assert_eq!(fs.file_identity(&child("a", &root)).unwrap(), Some(first));
    assert_eq!(
        super::rename_batch::rename_all(&root, &[("a", "a-longer-renamed-file")]),
        RenameOutcome::Done
    );
    assert_eq!(
        fs.file_identity(&child("a-longer-renamed-file", &root))
            .unwrap(),
        Some(first)
    );
    fs::create_dir(path.join("nested")).unwrap();
    fs.rename(
        &child("a-longer-renamed-file", &root),
        &child("renamed", &child("nested", &root)),
    )
    .unwrap();
    assert_eq!(
        fs.file_identity(&child("renamed", &child("nested", &root)))
            .unwrap(),
        Some(first)
    );
}

fn child(name: &str, root: &Location) -> Location {
    root.join(&ProviderPath::from_text(name)).unwrap()
}

#[allow(
    unsafe_code,
    reason = "the native test verifies the actual image filesystem"
)]
fn filesystem(path: &Path) -> String {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, GetVolumeInformationByHandleW,
    };

    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .unwrap();
    let mut name = [0_u16; 64];
    // SAFETY: the directory handle and filesystem-name output live through the query.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    let end = name.iter().position(|unit| *unit == 0).unwrap();
    String::from_utf16(&name[..end]).unwrap()
}

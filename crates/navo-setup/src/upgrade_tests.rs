// Windows tests for package staging, Update.exe replacement and apply arguments.

#![cfg(windows)]

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use navo_update::parse_version;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetFileSecurityW, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SetFileSecurityW,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

use super::{Root, Staged, apply_args, replace_update_exe, stage_package, writable};
use crate::payload::Payload;
use crate::probe::chain::resolve_path;
use crate::probe::pin::Pin;

/// Builds a pinned existing root.
fn pinned(root: &Path) -> Pin {
    let chain = resolve_path(root).expect("chain resolves");
    let id = chain.existing.last().expect("root exists").id;
    Pin::existing(root, id).expect("pin root")
}

/// Builds a valid full nupkg with the requested Squirrel entries.
fn payload(root: &Path, version: &str, squirrels: &[&str]) -> Payload {
    let setup = root.join("input.exe");
    fs::write(&setup, b"setup").expect("setup");
    let package = root.join(format!("NavoApp-{version}-full.nupkg"));
    let mut zip = zip::ZipWriter::new(File::create(&package).expect("package"));
    zip.start_file("package.nuspec", zip::write::SimpleFileOptions::default())
        .expect("nuspec entry");
    zip.write_all(nuspec(version).as_bytes()).expect("nuspec");
    for (index, name) in squirrels.iter().enumerate() {
        zip.start_file(
            format!("lib/{name}"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("squirrel entry");
        zip.write_all(format!("squirrel-{index}").as_bytes())
            .expect("squirrel bytes");
    }
    zip.finish().expect("finish package");
    Payload::from_files(&setup, &package, parse_version(version).expect("version"))
        .expect("payload")
}

/// The NuGet metadata shared by `sq.version` and the payload validator.
fn nuspec(version: &str) -> String {
    format!(
        "<package xmlns='urn:nuget'><metadata><id>NavoApp</id><version>{version}</version></metadata></package>"
    )
}

#[test]
fn staged_package_has_the_payload_hash() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let pin = pinned(temp.path());
    let staged = stage_package(&pin, &payload).expect("stage");
    let bytes = fs::read(&staged.path).expect("staged bytes");
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(&bytes)),
        payload.package_sha256
    );
    assert_eq!(
        staged.path.file_name().and_then(|name| name.to_str()),
        Some(payload.package_name.as_str())
    );
    assert!(
        !temp
            .path()
            .join("packages")
            .join(format!("{}.navo-partial", payload.package_name))
            .exists()
    );
}

#[test]
fn a_failed_package_move_removes_the_partial() {
    use std::os::windows::fs::OpenOptionsExt;

    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let packages = temp.path().join("packages");
    fs::create_dir(&packages).expect("packages");
    let final_path = packages.join(&payload.package_name);
    fs::write(&final_path, b"old").expect("old package");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&final_path)
        .expect("hold package");
    let pin = pinned(temp.path());
    assert!(stage_package(&pin, &payload).is_err());
    drop(held);
    assert_eq!(fs::read(&final_path).expect("old package"), b"old");
    assert!(
        !packages
            .join(format!("{}.navo-partial", payload.package_name))
            .exists()
    );
}

#[test]
fn a_leftover_partial_does_not_block_staging() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let packages = temp.path().join("packages");
    fs::create_dir(&packages).expect("packages");
    let partial = packages.join(format!("{}.navo-partial", payload.package_name));
    fs::write(&partial, b"stale").expect("stale partial");
    let pin = pinned(temp.path());
    let staged = stage_package(&pin, &payload).expect("stage");
    let bytes = fs::read(&staged.path).expect("staged bytes");
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(&bytes)),
        payload.package_sha256
    );
    assert!(!partial.exists());
}

#[test]
fn a_leftover_update_new_does_not_block_replacement() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    fs::write(temp.path().join("Update.exe"), b"old").expect("old update");
    let new = temp.path().join("Update.exe.navo-new");
    fs::write(&new, b"stale").expect("stale new");
    let pin = pinned(temp.path());
    replace_update_exe(&pin, &payload).expect("replace");
    assert_eq!(
        fs::read(temp.path().join("Update.exe")).expect("new update"),
        b"squirrel-0"
    );
    assert!(!new.exists());
}

#[test]
fn a_directory_partial_refuses_staging() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let packages = temp.path().join("packages");
    fs::create_dir(&packages).expect("packages");
    let partial = packages.join(format!("{}.navo-partial", payload.package_name));
    fs::create_dir(&partial).expect("directory partial");
    let pin = pinned(temp.path());
    assert!(stage_package(&pin, &payload).is_err());
    assert!(partial.is_dir());
}

#[test]
fn a_directory_update_new_refuses_replacement() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let update = temp.path().join("Update.exe");
    fs::write(&update, b"old").expect("old update");
    let new = temp.path().join("Update.exe.navo-new");
    fs::create_dir(&new).expect("directory new");
    let pin = pinned(temp.path());
    assert!(replace_update_exe(&pin, &payload).is_err());
    assert!(new.is_dir());
    assert_eq!(fs::read(&update).expect("old update"), b"old");
}

#[test]
fn an_existing_package_of_the_same_name_is_replaced() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let packages = temp.path().join("packages");
    fs::create_dir(&packages).expect("packages");
    fs::write(packages.join(&payload.package_name), b"old").expect("old package");
    let pin = pinned(temp.path());
    let staged = stage_package(&pin, &payload).expect("stage");
    let bytes = fs::read(&staged.path).expect("staged bytes");
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(&bytes)),
        payload.package_sha256
    );
}

#[test]
fn replace_update_exe_writes_one_squirrel_entry() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    fs::write(temp.path().join("Update.exe"), b"old").expect("old update");
    let pin = pinned(temp.path());
    let hash = replace_update_exe(&pin, &payload).expect("replace");
    assert_eq!(
        fs::read(temp.path().join("Update.exe")).expect("new update"),
        b"squirrel-0"
    );
    assert_eq!(hash, <[u8; 32]>::from(Sha256::digest(b"squirrel-0")));
    assert!(!temp.path().join("Update.exe.navo-new").exists());
}

#[test]
fn a_package_without_squirrel_refuses() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &[]);
    fs::write(temp.path().join("Update.exe"), b"old").expect("old update");
    let pin = pinned(temp.path());
    assert!(replace_update_exe(&pin, &payload).is_err());
    assert_eq!(
        fs::read(temp.path().join("Update.exe")).expect("old update"),
        b"old"
    );
    assert!(!temp.path().join("Update.exe.navo-new").exists());
}

#[test]
fn a_package_with_two_squirrel_entries_refuses() {
    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(
        temp.path(),
        "0.0.3",
        &["Squirrel.exe", "nested/Squirrel.exe"],
    );
    fs::write(temp.path().join("Update.exe"), b"old").expect("old update");
    let pin = pinned(temp.path());
    assert!(replace_update_exe(&pin, &payload).is_err());
    assert_eq!(
        fs::read(temp.path().join("Update.exe")).expect("old update"),
        b"old"
    );
    assert!(!temp.path().join("Update.exe.navo-new").exists());
}

#[test]
fn an_open_update_exe_keeps_the_old_file() {
    use std::os::windows::fs::OpenOptionsExt;

    let temp = tempfile::tempdir().expect("temp");
    let payload = payload(temp.path(), "0.0.3", &["Squirrel.exe"]);
    let update = temp.path().join("Update.exe");
    fs::write(&update, b"old").expect("old update");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&update)
        .expect("hold update");
    let pin = pinned(temp.path());
    assert!(replace_update_exe(&pin, &payload).is_err());
    drop(held);
    assert_eq!(fs::read(&update).expect("old update"), b"old");
    assert!(!temp.path().join("Update.exe.navo-new").exists());
}

#[test]
fn apply_args_matches_the_library_array_plus_log() {
    let root: Root = PathBuf::from(r"C:\Apps\Navo");
    let staged = Staged {
        path: root.join("packages").join("NavoApp-0.0.3-full.nupkg"),
    };
    let log = PathBuf::from(r"C:\Temp\run-velopack.log");
    let expected: Vec<OsString> = vec![
        root.join("Update.exe").into_os_string(),
        "apply".into(),
        "--package".into(),
        staged.path.clone().into_os_string(),
        "--silent".into(),
        "--norestart".into(),
        "--root".into(),
        root.clone().into_os_string(),
        "--packageDir".into(),
        root.join("packages").into_os_string(),
        "--log".into(),
        log.as_os_str().to_os_string(),
        "--verbose".into(),
    ];
    assert_eq!(apply_args(&root, &staged, &log), expected);
}

#[test]
fn a_read_only_root_is_not_writable() {
    let temp = tempfile::tempdir().expect("temp");
    let mut original = read_dacl(temp.path());
    deny_write(temp.path());
    let pin = pinned(temp.path());
    let result = writable(&pin);
    restore_dacl(temp.path(), &mut original);
    drop(pin);
    assert!(!result.expect("writable probe"));
}

/// Reads the current DACL so the test can restore it.
fn read_dacl(path: &Path) -> Vec<u8> {
    let path = wide(path);
    let mut needed = 0u32;
    // SAFETY: the path is NUL-terminated; a null buffer asks for the needed size.
    unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    assert!(needed > 0, "read DACL size");
    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: the path is NUL-terminated and `buffer` has `needed` writable bytes.
    let ok = unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    assert_ne!(ok, 0, "read DACL");
    buffer
}

/// Replaces the DACL with deny-write plus read for Everyone, so the pin can still open.
fn deny_write(path: &Path) {
    let sddl = wide_text("D:PAI(D;OICI;0x2;;;WD)(A;OICI;0x1600A9;;;WD)");
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let mut size = 0u32;
    // SAFETY: the SDDL string is NUL-terminated and the output pointers are writable.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            &mut size,
        )
    };
    assert_ne!(converted, 0, "convert SDDL");
    let path = wide(path);
    // SAFETY: the descriptor is live and the path is NUL-terminated.
    let applied = unsafe {
        SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    // SAFETY: the converted descriptor is owned by this test and no longer needed.
    unsafe { LocalFree(descriptor) };
    assert_ne!(applied, 0, "apply deny DACL");
}

/// Restores a DACL read before the test.
fn restore_dacl(path: &Path, descriptor: &mut [u8]) {
    let path = wide(path);
    // SAFETY: the path is NUL-terminated and `descriptor` is a self-relative security descriptor.
    let applied = unsafe {
        SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
        )
    };
    assert_ne!(applied, 0, "restore DACL");
}

/// A NUL-terminated UTF-16 path.
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A NUL-terminated UTF-16 string.
fn wide_text(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

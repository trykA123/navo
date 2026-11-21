//! Temp-folder tests of the Windows `RealDisk` contract: long paths, the read-only
//! attribute, sharing locks on `replace`, junctions and the no-replace rename.
// Windows only; clippy sees `cfg(test)` on the module, so test unwraps stay allowed.
#![cfg(windows)]

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use super::{Disk, RealDisk, ReplaceError};

fn put(path: &Path, text: &str) {
    let mut file = RealDisk.create_new(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.sync().unwrap();
}

fn read(path: &Path) -> String {
    let mut text = String::new();
    RealDisk
        .open_read(path)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

/// Builds a folder chain under `root` whose full path passes 300 characters.
fn deep_dir(root: &Path) -> PathBuf {
    let mut dir = root.to_path_buf();
    while dir.as_os_str().len() < 300 {
        dir.push("d".repeat(60));
        RealDisk.create_dir(&dir).unwrap();
    }
    dir
}

#[test]
fn every_step_works_on_a_path_past_300_characters() {
    let root = tempfile::tempdir().unwrap();
    let dir = deep_dir(root.path());
    let (staged, target, old) = (dir.join("s"), dir.join("t"), dir.join("o"));
    put(&target, "old");
    let mut log = RealDisk.open_append(&target).unwrap();
    log.write_all(b"+").unwrap();
    log.sync().unwrap();
    drop(log);
    put(&staged, "new");
    RealDisk.set_modified(&staged, 1_600_000_000_000).unwrap();
    let meta = RealDisk.meta(&staged).unwrap().unwrap();
    assert_eq!((meta.size, meta.modified_ms), (3, Some(1_600_000_000_000)));
    assert_eq!(read(&target), "old+");
    RealDisk.replace(&staged, &target, &old).unwrap();
    assert_eq!(read(&target), "new");
    assert_eq!(RealDisk.list(&dir).unwrap(), ["t"]);
    RealDisk.rename_new(&target, &staged).unwrap();
    RealDisk.remove_file(&staged).unwrap();
    assert!(RealDisk.meta(&staged).unwrap().is_none());
    RealDisk
        .remove_dir_all(&root.path().join("d".repeat(60)))
        .unwrap();
}

/// Sets or clears the read-only attribute; cleared again so the temp folder can be removed.
#[allow(
    clippy::permissions_set_readonly_false,
    reason = "Windows: this clears only the read-only attribute"
)]
fn set_read_only(path: &Path, on: bool) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(on);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn the_read_only_attribute_reads_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let (locked, open) = (dir.path().join("ro"), dir.path().join("rw"));
    put(&locked, "x");
    put(&open, "x");
    set_read_only(&locked, true);
    let read_only = RealDisk.meta(&locked).unwrap().unwrap().read_only;
    set_read_only(&locked, false);
    assert!(read_only);
    assert!(!RealDisk.meta(&open).unwrap().unwrap().read_only);
}

#[test]
fn a_target_held_without_sharing_is_unchanged_with_error_32() {
    let dir = tempfile::tempdir().unwrap();
    let (staged, target, old) = (
        dir.path().join("s"),
        dir.path().join("t"),
        dir.path().join("o"),
    );
    put(&target, "old");
    put(&staged, "new");
    let holder = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&target)
        .unwrap();
    let result = RealDisk.replace(&staged, &target, &old);
    drop(holder);
    assert!(
        matches!(&result, Err(ReplaceError::Unchanged(e)) if e.raw_os_error() == Some(32)),
        "{result:?}"
    );
    assert_eq!((read(&target), read(&staged)), ("old".into(), "new".into()));
    assert!(RealDisk.meta(&old).unwrap().is_none());
}

#[test]
fn a_target_open_with_all_share_flags_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let (staged, target, old) = (
        dir.path().join("s"),
        dir.path().join("t"),
        dir.path().join("o"),
    );
    put(&target, "old");
    put(&staged, "new");
    let mut reader = File::open(&target).unwrap();
    RealDisk.replace(&staged, &target, &old).unwrap();
    let mut held = String::new();
    reader.read_to_string(&mut held).unwrap();
    drop(reader);
    assert_eq!(held, "old", "the open handle keeps the old file");
    assert_eq!(read(&target), "new");
    assert_eq!(RealDisk.list(dir.path()).unwrap(), ["t"]);
}

#[test]
fn rename_new_refuses_an_existing_name_and_keeps_both_files() {
    let dir = tempfile::tempdir().unwrap();
    let (from, to) = (dir.path().join("a"), dir.path().join("b"));
    put(&from, "a");
    put(&to, "b");
    assert_eq!(
        RealDisk.rename_new(&from, &to).unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!((read(&from), read(&to)), ("a".into(), "b".into()));
}

/// True when `dir` is on NTFS; otherwise prints that the test of `feature` is skipped and why,
/// or panics when `SIDEWISE_REQUIRE_NTFS=1` (the VM gate). Junctions, WOF and custom reparse
/// tags need NTFS; TEMP may sit on exFAT or FAT.
#[expect(clippy::print_stderr, reason = "a skipped test prints why")]
fn on_ntfs(dir: &Path, feature: &str) -> bool {
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)
        .unwrap();
    let name = crate::native::identity::filesystem_name(&handle).unwrap();
    let ntfs = name.eq_ignore_ascii_case("NTFS");
    let required = std::env::var_os("SIDEWISE_REQUIRE_NTFS").is_some_and(|value| value == "1");
    assert!(
        ntfs || !required,
        "{feature} needs NTFS, the temp volume is {name}"
    );
    if !ntfs {
        eprintln!("SKIPPED {feature}: needs NTFS, the temp volume is {name}");
    }
    ntfs
}

#[test]
fn a_junction_or_a_relative_path_is_refused() {
    let error = RealDisk.meta(Path::new("relative.txt")).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    let dir = tempfile::tempdir().unwrap();
    if !on_ntfs(dir.path(), "junction") {
        return;
    }
    let (real, link) = (dir.path().join("real"), dir.path().join("link"));
    RealDisk.create_dir(&real).unwrap();
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&real)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    let error = RealDisk.meta(&link).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

/// WOF-compresses `path` (`compact /EXE:xpress4k`); returns what `fsutil` sees, for messages.
fn wof_compress(path: &Path) -> String {
    let run = |program: &str, args: &[&str]| {
        let done = std::process::Command::new(program)
            .args(args)
            .arg(path)
            .output()
            .unwrap();
        assert!(done.status.success() || program == "fsutil", "{done:?}");
        String::from_utf8_lossy(&done.stdout).into_owned()
    };
    run("compact", &["/C", "/EXE:xpress4k"]);
    run("fsutil", &["reparsepoint", "query"])
}

/// WOF hides its reparse point (fsutil: error 4390), so the file must read its real bytes;
/// were the tag ever visible, the file would have to be refused as a placeholder instead.
#[test]
fn a_wof_compressed_file_reads_its_real_bytes_or_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    if !on_ntfs(dir.path(), "WOF compression") {
        return;
    }
    let packed = dir.path().join("packed.txt");
    let text = "sidewise ".repeat(8192);
    put(&packed, &text);
    let fsutil = wof_compress(&packed);
    match RealDisk.open_read(&packed) {
        Ok(mut file) => {
            let mut seen = String::new();
            file.read_to_string(&mut seen).unwrap();
            assert!(seen == text, "WOF bytes differ; fsutil: {fsutil}");
            assert_eq!(RealDisk.meta(&packed).unwrap().unwrap().size, 73_728);
        }
        Err(error) => assert!(error.to_string().contains("placeholder"), "{error}"),
    }
}

/// Puts a third-party, non-link reparse point (tag 0x1234, as a sync or HSM filter would)
/// on `path`. No filter owns the tag, so only a no-follow open reaches the file.
#[allow(unsafe_code, reason = "FSCTL_SET_REPARSE_POINT is a Win32 call")]
fn tag_as_placeholder(path: &Path) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
    let file = OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap();
    // REPARSE_GUID_DATA_BUFFER: tag, data length, reserved, a 16-byte GUID, then the data.
    let mut buffer = 0x0000_1234_u32.to_le_bytes().to_vec();
    buffer.extend(4_u16.to_le_bytes());
    buffer.extend(0_u16.to_le_bytes());
    buffer.extend(*b"sidewise-test-id");
    buffer.extend(*b"stub");
    let mut returned = 0_u32;
    // SAFETY: the handle, the input buffer and `returned` live through the call; no output.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
}

#[test]
fn a_file_with_a_non_link_reparse_tag_is_refused_as_a_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    if !on_ntfs(dir.path(), "a non-link reparse tag") {
        return;
    }
    let stub = dir.path().join("cloud.txt");
    put(&stub, "stub bytes, not the real file");
    tag_as_placeholder(&stub);
    let errors = [
        RealDisk.open_read(&stub).err().unwrap(),
        RealDisk.meta(&stub).unwrap_err(),
        RealDisk.set_modified(&stub, 1_600_000_000_000).unwrap_err(),
    ];
    for error in errors {
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{error}");
        assert!(error.to_string().contains("placeholder"), "{error}");
    }
}

#[test]
fn device_paths_are_refused() {
    for device in [r"\\.\C:\Windows", r"\\?\GLOBALROOT\Device\Null", r"\\?\C:"] {
        let error = RealDisk.meta(Path::new(device)).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{device}: {error}");
    }
}

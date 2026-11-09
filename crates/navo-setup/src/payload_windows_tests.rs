//! Windows tests for work-folder ACLs, held extraction, cleanup and log rotation.

use super::super::tests::file_payload;
use super::*;
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE,
    Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
    DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl, INHERITED_ACE,
    SE_DACL_PROTECTED,
};

/// Creates a deterministic isolated work folder for tests.
fn work(root: &Path) -> io::Result<Work> {
    Work::create_in(root, &mut security::random_name)
}

/// An existing name is never reused or deleted; the next generated name succeeds.
#[test]
fn existing_work_folder_retries_with_a_new_name() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let occupied = root.path().join(format!("NavoSetup-{}", "a".repeat(32)));
    fs::create_dir(&occupied)?;
    fs::write(occupied.join("keep"), b"keep")?;
    let mut names = ["a".repeat(32), "b".repeat(32)].into_iter();
    let work = Work::create_in(root.path(), &mut || {
        names
            .next()
            .ok_or_else(|| io::Error::other("names exhausted"))
    })?;
    assert!(work.path.ends_with(format!("NavoSetup-{}", "b".repeat(32))));
    assert!(work.cleanup(None)?);
    assert_eq!(fs::read(occupied.join("keep"))?, b"keep");
    Ok(())
}

/// A live extraction blocks writes, then cleanup closes it and removes only its work folder.
#[test]
fn cleanup_with_live_extracted_removes_only_work() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let payload = file_payload(root.path())?;
    let work = work(root.path())?;
    let directory = work.path.clone();
    let logs = root.path().join("NavoSetup-logs");
    fs::create_dir(&logs)?;
    fs::write(logs.join("20261109T120000000Z.log"), b"keep")?;
    let extracted = payload.extract(&work)?;
    assert!(File::open(&extracted.path).is_ok());
    assert_eq!(
        OpenOptions::new()
            .write(true)
            .open(&extracted.path)
            .err()
            .and_then(|e| e.raw_os_error()),
        Some(32)
    );
    assert!(fs::rename(&extracted.path, directory.join("renamed")).is_err());
    assert!(fs::remove_file(&extracted.path).is_err());
    assert!(work.cleanup(Some(extracted))?);
    assert!(!directory.exists());
    assert!(logs.join("20261109T120000000Z.log").exists());
    Ok(())
}

/// Extraction refuses a hash mismatch before returning an executable handle.
#[test]
fn hash_mismatch_refuses_extraction() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let mut payload = file_payload(root.path())?;
    payload.setup_sha256[0] ^= 1;
    let work = work(root.path())?;
    assert!(payload.extract(&work).is_err());
    assert!(work.cleanup(None)?);
    Ok(())
}

/// Cleanup retains a replacement file while its recorded predecessor still exists elsewhere.
#[test]
fn cleanup_keeps_a_file_with_a_different_id() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let payload = file_payload(root.path())?;
    let work = work(root.path())?;
    let extracted = payload.extract(&work)?;
    let path = extracted.path.clone();
    drop(extracted);
    fs::rename(&path, root.path().join("original.exe"))?;
    fs::write(&path, b"replacement")?;
    assert!(!work.cleanup(None)?);
    assert_eq!(fs::read(path)?, b"replacement");
    Ok(())
}

/// A protected directory DACL has exactly two explicit allow ACEs: user and SYSTEM.
#[test]
fn work_dacl_grants_only_user_and_system() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let work = work(root.path())?;
    let mut descriptor = ptr::null_mut();
    let mut acl = ptr::null_mut();
    // SAFETY: path is NUL-terminated; all output pointers are writable or optional nulls.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide(&work.path).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, 0);
    let _allocation = security::Local(descriptor);
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: the successful query allocated a valid descriptor and output fields are writable.
    let queried = unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) };
    assert_ne!(queried, 0);
    assert_ne!(control & SE_DACL_PROTECTED, 0);
    assert!(!acl.is_null());
    // SAFETY: GetNamedSecurityInfoW returned the live ACL inside descriptor.
    assert_eq!(unsafe { (*acl).AceCount }, 2);
    let mut sids = Vec::new();
    for index in 0..2 {
        sids.push(allowed_sid(acl, index)?);
    }
    sids.sort();
    let mut expected = vec![security::user_sid()?, "S-1-5-18".to_owned()];
    expected.sort();
    assert_eq!(sids, expected);
    assert!(work.cleanup(None)?);
    Ok(())
}

/// Reads one explicit full-access allow ACE from an ACL returned by Windows.
fn allowed_sid(acl: *const windows_sys::Win32::Security::ACL, index: u32) -> io::Result<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: acl is live and index is within the verified two-ACE count.
    if unsafe { GetAce(acl, index, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetAce returned an ACE; its header is readable before interpreting its type.
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    assert_eq!(ace.Header.AceType, 0);
    assert_eq!(u32::from(ace.Header.AceFlags) & INHERITED_ACE, 0);
    assert_eq!(ace.Mask, 0x001f_01ff);
    security::sid_text(ptr::from_ref(&ace.SidStart).cast_mut().cast())
}

/// Rotation skips reparse points and read-only old logs while deleting other expired files.
#[test]
fn rotation_skips_reparse_and_read_only_logs() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let logs = root.path().join("logs");
    fs::create_dir(&logs)?;
    for day in 1..=7 {
        for suffix in [".log", "-velopack.log", "-velopack.log.old"] {
            fs::write(
                logs.join(format!("202611{day:02}T120000000Z{suffix}")),
                b"log",
            )?;
        }
    }
    let target = root.path().join("target");
    fs::create_dir(&target)?;
    fs::write(target.join("keep"), b"keep")?;
    let link = logs.join("20261001T120000000Z.log");
    create_junction(&link, &target)?;
    let read_only = logs.join("20261101T120000000Z.log");
    let mut permissions = fs::metadata(&read_only)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions)?;
    rotate_logs(&logs)?;
    assert_eq!(fs::read_dir(&logs)?.count(), 17);
    assert_eq!(fs::read(&read_only)?, b"log");
    assert_ne!(
        fs::symlink_metadata(&link)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT,
        0
    );
    assert_eq!(fs::read(target.join("keep"))?, b"keep");
    assert!(!logs.join("20261102T120000000Z.log").exists());
    assert!(logs.join("20261103T120000000Z-velopack.log.old").exists());
    for day in 1..=7 {
        fs::write(
            target.join(format!("202611{day:02}T120000000Z.log")),
            b"keep",
        )?;
    }
    rotate_logs(&link)?;
    assert!(target.join("20261101T120000000Z.log").exists());
    assert_eq!(fs::read(target.join("keep"))?, b"keep");
    let mut permissions = fs::metadata(&read_only)?.permissions();
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "Windows only: clears the read-only attribute so the temp folder can be removed"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(read_only, permissions)?;
    Ok(())
}

/// Creates an NTFS junction without requiring administrator or symlink privileges.
fn create_junction(link: &Path, target: &Path) -> io::Result<()> {
    use windows_sys::Win32::{Foundation::GENERIC_WRITE, System::IO::DeviceIoControl};
    fs::create_dir(link)?;
    let file = open(link, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE)?;
    let target = fs::canonicalize(target)?;
    let mut name: Vec<u16> = target.as_os_str().encode_wide().collect();
    if name.starts_with(&[92, 92, 63, 92]) {
        name.drain(..4);
    }
    let substitute: Vec<u16> = r"\??\".encode_utf16().chain(name.iter().copied()).collect();
    let sub_length = u16::try_from(substitute.len() * 2).map_err(io::Error::other)?;
    let print_length = u16::try_from(name.len() * 2).map_err(io::Error::other)?;
    let data_length = 12u16
        .checked_add(sub_length)
        .and_then(|n| n.checked_add(print_length))
        .ok_or_else(|| io::Error::other("junction target too long"))?;
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&0xa000_0003u32.to_le_bytes());
    for value in [data_length, 0, 0, sub_length, sub_length + 2, print_length] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for unit in substitute
        .into_iter()
        .chain(Some(0))
        .chain(name)
        .chain(Some(0))
    {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    let length = u32::try_from(buffer.len()).map_err(io::Error::other)?;
    let mut returned = 0;
    // SAFETY: the mount-point buffer matches FSCTL_SET_REPARSE_POINT; file is a writable directory.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            0x0009_00a4,
            buffer.as_ptr().cast(),
            length,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

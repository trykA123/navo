//! Derives the PDF helper's AppContainer SID from Navo's fixed name (its profile is registered in
//! `container_win.rs`) and
//! grants it read+execute on one file when it lacks access (S2c1). The rules live in `appcontainer_policy.rs`.

use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS,
    NO_MULTIPLE_TRUSTEE, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::DeriveAppContainerSidFromAppContainerName;
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, EqualSid, FreeSid, GetKernelObjectSecurity,
    InitializeSecurityDescriptor, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
    SE_DACL_AUTO_INHERIT_REQ, SE_DACL_AUTO_INHERITED, SE_DACL_PROTECTED, SECURITY_DESCRIPTOR,
    SetKernelObjectSecurity, SetSecurityDescriptorControl, SetSecurityDescriptorDacl,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, READ_CONTROL, ReOpenFile, WRITE_DAC,
};

use super::appcontainer_policy::{
    ALL_APP_PACKAGES, AceView, CONTAINER_NAME, DaclSpan, ERROR_INVALID_ACL, EVERYONE, GrantError,
    GrantOutcome, GrantStep, READ_EXECUTE, Trustee, already_granted, check_plain_file, locate_dacl,
    parse_dacl,
};

/// `SECURITY_DESCRIPTOR_REVISION` (winnt.h).
const SD_REVISION: u32 = 1;

/// Size queries before giving up when the DACL keeps growing between the query and the read.
const READ_ATTEMPTS: usize = 3;

/// The helper's AppContainer SID; freed with `FreeSid` on drop.
pub(crate) struct ContainerSid(PSID);

/// Memory Windows allocated with `LocalAlloc`; freed with `LocalFree` on drop.
struct LocalMem(*mut c_void);

impl Drop for LocalMem {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a Win32 call that hands LocalAlloc memory to the caller.
        unsafe { LocalFree(self.0) };
    }
}

impl Drop for ContainerSid {
    fn drop(&mut self) {
        // SAFETY: the SID came from DeriveAppContainerSidFromAppContainerName, which says FreeSid.
        unsafe { FreeSid(self.0) };
    }
}

impl ContainerSid {
    /// Derives the SID from `CONTAINER_NAME`. Fails at `DeriveSid` with the HRESULT.
    pub(crate) fn derive() -> Result<Self, GrantError> {
        let name: Vec<u16> = CONTAINER_NAME.encode_utf16().chain([0]).collect();
        let mut sid: PSID = ptr::null_mut();
        // SAFETY: `name` is NUL-terminated and `sid` is a live out-pointer.
        let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &raw mut sid) };
        if hr < 0 || sid.is_null() {
            return Err(GrantError::new(GrantStep::DeriveSid, None, hr));
        }
        Ok(Self(sid))
    }

    /// The raw SID for `SECURITY_CAPABILITIES`; valid while `self` lives.
    pub(crate) fn as_psid(&self) -> PSID {
        self.0
    }

    /// The SID as text (`S-1-15-2-…`) for logs. Fails at `FormatSid` with the Win32 error.
    pub(crate) fn to_sddl(&self) -> Result<String, GrantError> {
        let mut text: *mut u16 = ptr::null_mut();
        // SAFETY: the SID is live and `text` is a live out-pointer.
        if unsafe { ConvertSidToStringSidW(self.0, &raw mut text) } == 0 {
            return Err(GrantError::new(GrantStep::FormatSid, None, last_error()));
        }
        let owned = LocalMem(text.cast());
        // SAFETY: Windows wrote a NUL-terminated string at `text`, owned by `owned`.
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        // SAFETY: the `len` units before the NUL are initialised and live while `owned` is.
        let units = unsafe { std::slice::from_raw_parts(text, len) };
        let sddl = String::from_utf16_lossy(units);
        drop(owned);
        Ok(sddl)
    }
}

/// A file's security descriptor, read through its handle into a buffer Navo owns, and where its
/// DACL lies in it. The `u32` buffer keeps the DWORD-aligned DACL aligned for Win32.
struct FileDacl {
    descriptor: Vec<u32>,
    span: DaclSpan,
}

impl FileDacl {
    /// The DACL's bytes; `locate_dacl` checked that they lie inside the buffer.
    fn bytes(&self) -> Result<&[u8], i32> {
        let range = self.span.range.clone();
        as_bytes(&self.descriptor)
            .get(range)
            .ok_or(ERROR_INVALID_ACL)
    }

    /// The descriptor's DACL control bits that a write must keep: protected and auto-inherited.
    fn dacl_control(&self) -> Result<u16, i32> {
        let bytes = as_bytes(&self.descriptor);
        let control = bytes.get(2..4).ok_or(ERROR_INVALID_ACL)?;
        let control = u16::from_le_bytes([control[0], control[1]]);
        Ok(control & (SE_DACL_PROTECTED | SE_DACL_AUTO_INHERITED))
    }

    /// The DACL for Win32 calls, valid while `self` lives.
    fn acl(&self) -> Result<*const ACL, i32> {
        self.bytes().map(|bytes| bytes.as_ptr().cast())
    }
}

/// `words` seen as bytes.
fn as_bytes(words: &[u32]) -> &[u8] {
    // SAFETY: every byte of a `u32` is initialised and any value is a valid `u8`; the view borrows
    // `words` and covers exactly its bytes.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), size_of_val(words)) }
}

/// Grants `sid` read+execute on `path` with one explicit, non-inherited ACE when the DACL does not
/// already give it. Checks with READ_CONTROL only and asks for WRITE_DAC by reopening the same
/// handle, so the file cannot be swapped between check and write. Refuses reparse points and folders.
pub(crate) fn grant_read_execute(
    path: &Path,
    sid: &ContainerSid,
) -> Result<GrantOutcome, GrantError> {
    let fail = |step, code| GrantError::new(step, Some(path), code);
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| fail(GrantStep::OpenFile, e.raw_os_error().unwrap_or(-1)))?;
    let attributes = file
        .metadata()
        .map_err(|e| fail(GrantStep::CheckTarget, e.raw_os_error().unwrap_or(-1)))?
        .file_attributes();
    check_plain_file(attributes).map_err(|code| fail(GrantStep::CheckTarget, code))?;
    let current = read_dacl(&file).map_err(|code| fail(GrantStep::ReadDacl, code))?;
    let aces = ace_views(&current, sid).map_err(|code| fail(GrantStep::ScanDacl, code))?;
    if already_granted(&aces) {
        return Ok(GrantOutcome::AlreadyGranted);
    }
    let writable = reopen_for_write(&file).map_err(|code| fail(GrantStep::OpenForWrite, code))?;
    let merged = current
        .acl()
        .and_then(|acl| merge_grant(acl, sid))
        .map_err(|code| fail(GrantStep::BuildDacl, code))?;
    current
        .dacl_control()
        .and_then(|control| write_dacl(&writable, merged.0.cast(), control))
        .map_err(|code| fail(GrantStep::WriteDacl, code))?;
    Ok(GrantOutcome::Added)
}

/// Reopens the same file object as `file` with WRITE_DAC added. Fails with the Win32 error, for
/// example access denied when the user may not change the file's permissions.
fn reopen_for_write(file: &File) -> Result<File, i32> {
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    // SAFETY: the handle is live; ReOpenFile returns a new handle the caller owns.
    let handle = unsafe {
        ReOpenFile(
            file.as_raw_handle(),
            READ_CONTROL | WRITE_DAC,
            share,
            FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(last_error());
    }
    // SAFETY: `handle` is a fresh, valid file handle owned by nobody else.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// The well-known SID written as `sddl` (e.g. `S-1-1-0`), owned by `LocalMem`. Fails with the
/// Win32 error.
fn well_known_sid(sddl: &str) -> Result<LocalMem, i32> {
    let text: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
    let mut sid: PSID = ptr::null_mut();
    // SAFETY: `text` is NUL-terminated and `sid` is a live out-pointer.
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &raw mut sid) } == 0 {
        return Err(last_error());
    }
    Ok(LocalMem(sid))
}

/// Reads the DACL and its protection bit through `file`. The DACL is bounded by the bytes Windows
/// wrote into Navo's buffer before any slice is made. Fails with the Win32 error; a malformed,
/// missing or NULL DACL fails with `ERROR_INVALID_ACL`.
fn read_dacl(file: &File) -> Result<FileDacl, i32> {
    let (descriptor, written) = read_descriptor(file)?;
    let bytes = as_bytes(&descriptor)
        .get(..written)
        .ok_or(ERROR_INVALID_ACL)?;
    let span = locate_dacl(bytes)?;
    Ok(FileDacl { descriptor, span })
}

/// Reads `file`'s self-relative security descriptor (DACL part) through the same handle into a
/// buffer Navo owns: one size query, then the read. Returns the buffer and the bytes written.
fn read_descriptor(file: &File) -> Result<(Vec<u32>, usize), i32> {
    let mut words: Vec<u32> = Vec::new();
    for _ in 0..READ_ATTEMPTS {
        let capacity =
            u32::try_from(size_of_val(words.as_slice())).map_err(|_| ERROR_INVALID_ACL)?;
        let buffer = if words.is_empty() {
            ptr::null_mut()
        } else {
            words.as_mut_ptr().cast()
        };
        let mut needed = 0u32;
        // SAFETY: the handle is live with READ_CONTROL; Windows writes at most `capacity` bytes at
        // `buffer`, which `words` owns, and `needed` is a live out-pointer.
        let ok = unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                DACL_SECURITY_INFORMATION,
                buffer,
                capacity,
                &raw mut needed,
            )
        };
        if ok != 0 {
            return Ok((words, needed.min(capacity) as usize));
        }
        let error = last_error();
        if error != ERROR_INSUFFICIENT_BUFFER as i32 || needed <= capacity {
            return Err(error);
        }
        words = vec![0; (needed as usize).div_ceil(4)];
    }
    Err(ERROR_INSUFFICIENT_BUFFER as i32)
}

/// Describes every ACE in `current`'s DACL, naming allow and deny ACEs for `sid`, ALL APPLICATION
/// PACKAGES or Everyone. A malformed ACL fails with `ERROR_INVALID_ACL`, others with the Win32 error.
fn ace_views(current: &FileDacl, sid: &ContainerSid) -> Result<Vec<AceView>, i32> {
    let packages = well_known_sid(ALL_APP_PACKAGES)?;
    let everyone = well_known_sid(EVERYONE)?;
    parse_dacl(current.bytes()?, |known, ace_sid| {
        let other: PSID = match known {
            Trustee::Container => sid.0,
            Trustee::AllAppPackages => packages.0,
            Trustee::Everyone => everyone.0,
            Trustee::Other | Trustee::Unknown => return false,
        };
        // SAFETY: `parse_dacl` passes a SID it checked as IsValidSid and GetLengthSid would
        // (revision 1, at most 15 sub-authorities, its whole length inside the ACE); EqualSid
        // only reads it. `other` is a live SID.
        unsafe { EqualSid(ace_sid.as_ptr().cast_mut().cast(), other) != 0 }
    })
}

/// Builds a new ACL: `dacl` plus an explicit read+execute allow for `sid`. Fails with the Win32 error.
fn merge_grant(dacl: *const ACL, sid: &ContainerSid) -> Result<LocalMem, i32> {
    let access = EXPLICIT_ACCESS_W {
        grfAccessPermissions: READ_EXECUTE,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.0.cast(),
        },
    };
    let mut merged: *mut ACL = ptr::null_mut();
    // SAFETY: one live entry, a live old ACL and a live out-pointer; the SID outlives the call.
    let status = unsafe { SetEntriesInAclW(1, &raw const access, dacl, &raw mut merged) };
    if status != 0 {
        return Err(status as i32);
    }
    Ok(LocalMem(merged.cast()))
}

/// Writes `dacl` through `file` as given, with the DACL `control` bits read before. The kernel
/// stores it unchanged: inherited ACEs stay inherited and none are added again. `SetSecurityInfo`
/// would re-derive them from the parent and keep a legacy DACL's ACEs as explicit copies. The
/// kernel keeps the auto-inherited bit only when the write requests it.
fn write_dacl(file: &File, dacl: *const ACL, control: u16) -> Result<(), i32> {
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let sd: PSECURITY_DESCRIPTOR = (&raw mut descriptor).cast();
    let bits = SE_DACL_PROTECTED | SE_DACL_AUTO_INHERITED | SE_DACL_AUTO_INHERIT_REQ;
    let control = if control & SE_DACL_AUTO_INHERITED == 0 {
        control
    } else {
        control | SE_DACL_AUTO_INHERIT_REQ
    };
    // SAFETY: `sd` points at a live absolute descriptor that outlives every call here; `dacl` is a
    // live ACL it only points at, and the handle has WRITE_DAC.
    let ok = unsafe {
        InitializeSecurityDescriptor(sd, SD_REVISION) != 0
            && SetSecurityDescriptorDacl(sd, 1, dacl, 0) != 0
            && SetSecurityDescriptorControl(sd, bits, control) != 0
            && SetKernelObjectSecurity(file.as_raw_handle(), DACL_SECURITY_INFORMATION, sd) != 0
    };
    if ok { Ok(()) } else { Err(last_error()) }
}

/// The calling thread's last Win32 error.
fn last_error() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::{
        GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo,
    };
    use windows_sys::Win32::Security::{
        ACL_SIZE_INFORMATION, AclSizeInformation, GetAclInformation, GetSecurityDescriptorControl,
        INHERITED_ACE, PROTECTED_DACL_SECURITY_INFORMATION, UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

    /// Opens a file or folder for DACL reads and writes and reads its DACL.
    fn open_dacl(path: &Path) -> (File, FileDacl) {
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL | WRITE_DAC)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .unwrap();
        let dacl = read_dacl(&file).unwrap();
        (file, dacl)
    }

    fn control_of(path: &Path) -> u16 {
        open_dacl(path).1.dacl_control().unwrap()
    }

    /// The raw ACE flags as stored, in order.
    fn raw_flags(path: &Path, sid: &ContainerSid) -> Vec<u8> {
        let dacl = open_dacl(path).1;
        let aces = ace_views(&dacl, sid).unwrap();
        aces.iter().map(|ace| ace.flags).collect()
    }

    /// Rewrites `path`'s DACL through `SetSecurityInfo`, which marks it auto-inherited, protected
    /// when `protect` holds. Files created later in a folder rewritten so store real inherited flags.
    fn rewrite_with_set_security_info(path: &Path, protect: bool) {
        let (file, dacl) = open_dacl(path);
        let inherit = if protect {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        // SAFETY: the handle has WRITE_DAC and the DACL is live; only the DACL is set.
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | inherit,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl.acl().unwrap(),
                ptr::null(),
            )
        };
        assert_eq!(status, 0);
    }

    /// A temp folder whose files inherit through an auto-inherited DACL, as under Program Files.
    fn auto_inherited_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        rewrite_with_set_security_info(dir.path(), false);
        dir
    }

    /// Reads back `(flags, mask)` of the allow ACEs for `sid` on `path`, and the total ACE count.
    fn read_back(path: &Path, sid: &ContainerSid) -> (Vec<(u8, u32)>, u32) {
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL)
            .open(path)
            .unwrap();
        let dacl = read_dacl(&file).unwrap();
        let mut info = ACL_SIZE_INFORMATION::default();
        let bytes = size_of::<ACL_SIZE_INFORMATION>() as u32;
        // SAFETY: the DACL is live and `info` a live buffer of `bytes` bytes.
        let ok = unsafe {
            GetAclInformation(
                dacl.acl().unwrap(),
                (&raw mut info).cast(),
                bytes,
                AclSizeInformation,
            )
        };
        assert_ne!(ok, 0);
        let own = ace_views(&dacl, sid)
            .unwrap()
            .into_iter()
            .filter(|ace| ace.ace_type == 0 && ace.trustee == Trustee::Container)
            .map(|ace| (ace.flags, ace.mask))
            .collect();
        (own, info.AceCount)
    }

    #[test]
    fn the_container_sid_is_stable_and_an_appcontainer_sid() {
        let first = ContainerSid::derive().unwrap().to_sddl().unwrap();
        let second = ContainerSid::derive().unwrap().to_sddl().unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("S-1-15-2-"), "{first}");
    }

    #[test]
    fn a_grant_adds_one_explicit_read_execute_ace_and_a_second_adds_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pdfium.dll");
        std::fs::write(&path, b"x").unwrap();
        let sid = ContainerSid::derive().unwrap();
        let (before, count_before) = read_back(&path, &sid);
        assert!(before.is_empty());
        let control = control_of(&path);

        assert_eq!(grant_read_execute(&path, &sid), Ok(GrantOutcome::Added));
        let (after, count_after) = read_back(&path, &sid);
        assert_eq!(after, [(0, READ_EXECUTE)]);
        assert_eq!(count_after, count_before + 1);
        assert_eq!(control_of(&path), control);

        assert_eq!(
            grant_read_execute(&path, &sid),
            Ok(GrantOutcome::AlreadyGranted)
        );
        let (again, count_again) = read_back(&path, &sid);
        assert_eq!(again, [(0, READ_EXECUTE)]);
        assert_eq!(count_again, count_after);
    }

    #[test]
    fn a_protected_dacl_stays_protected_and_gains_only_the_grant() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pdfium.dll");
        std::fs::write(&path, b"x").unwrap();
        rewrite_with_set_security_info(&path, true);
        let sid = ContainerSid::derive().unwrap();
        let (_, count_before) = read_back(&path, &sid);
        assert_ne!(control_of(&path) & SE_DACL_PROTECTED, 0);

        assert_eq!(grant_read_execute(&path, &sid), Ok(GrantOutcome::Added));
        let (after, count_after) = read_back(&path, &sid);
        assert_eq!(after, [(0, READ_EXECUTE)]);
        assert_eq!(count_after, count_before + 1);
        assert_ne!(control_of(&path) & SE_DACL_PROTECTED, 0);
    }

    #[test]
    fn inherited_aces_stay_inherited_and_the_grant_is_the_only_explicit_ace() {
        let dir = auto_inherited_dir();
        let path = dir.path().join("pdfium.dll");
        std::fs::write(&path, b"x").unwrap();
        let sid = ContainerSid::derive().unwrap();
        let before = raw_flags(&path, &sid);
        assert!(!before.is_empty());
        assert!(
            before
                .iter()
                .all(|&flags| u32::from(flags) & INHERITED_ACE != 0),
            "{before:?}"
        );
        let control = control_of(&path);

        assert_eq!(grant_read_execute(&path, &sid), Ok(GrantOutcome::Added));
        let mut expected = vec![0];
        expected.extend(&before);
        assert_eq!(raw_flags(&path, &sid), expected);
        assert_eq!(control_of(&path), control);
    }

    #[test]
    fn a_missing_file_fails_at_open_file_and_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.dll");
        let sid = ContainerSid::derive().unwrap();
        let error = grant_read_execute(&path, &sid).unwrap_err();
        assert_eq!(error.step, GrantStep::OpenFile);
        assert_eq!(error.target.as_deref(), Some(path.as_path()));
        assert_eq!(error.code, 2);
    }

    #[test]
    fn the_dacl_in_navo_memory_matches_the_one_get_security_info_returns() {
        let dir = auto_inherited_dir();
        let path = dir.path().join("pdfium.dll");
        std::fs::write(&path, b"x").unwrap();
        let sid = ContainerSid::derive().unwrap();
        assert_eq!(grant_read_execute(&path, &sid), Ok(GrantOutcome::Added));
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL)
            .open(&path)
            .unwrap();
        let ours = read_dacl(&file).unwrap();
        let (mut dacl, mut descriptor) = (ptr::null_mut::<ACL>(), ptr::null_mut());
        // SAFETY: the handle is live and the out-pointers are live; only the DACL is asked.
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &raw mut dacl,
                ptr::null_mut(),
                &raw mut descriptor,
            )
        };
        assert_eq!(status, 0);
        let descriptor = LocalMem(descriptor);
        // SAFETY: Windows returned a valid DACL inside the live descriptor.
        let theirs =
            unsafe { std::slice::from_raw_parts(dacl.cast::<u8>(), usize::from((*dacl).AclSize)) };
        assert_eq!(ours.bytes().unwrap(), theirs);
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the descriptor is live and both out-pointers are live.
        let ok = unsafe {
            GetSecurityDescriptorControl(descriptor.0, &raw mut control, &raw mut revision)
        };
        assert_ne!(ok, 0);
        assert_eq!(ours.span.protected, control & SE_DACL_PROTECTED != 0);
    }
}

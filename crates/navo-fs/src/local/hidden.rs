use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, IsValidAcl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_SYSTEM,
    FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES, FileBasicInfo, GetFileInformationByHandleEx,
    READ_CONTROL, SetFileInformationByHandle, WRITE_DAC,
};

const PRIVATE_DACL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

pub(super) fn set(path: &Path, expected: crate::FileIdentity) -> io::Result<()> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | READ_CONTROL | WRITE_DAC)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if super::identity::from_file(&file)? != Some(expected) {
        return Err(io::Error::other("retention container identity changed"));
    }
    match apply(&file) {
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => return Err(error),
        result => warn_failed(expected, result),
    }
    super::vault_protection::require_private(super::win_owner::keeps_acls(&file), || protect(&file))
}

fn warn_failed(identity: crate::FileIdentity, result: io::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(
            volume_id = identity.volume,
            file_id = ?identity.file,
            error_kind = ?error.kind(),
            "retention container hiding failed"
        );
    }
}

#[allow(
    unsafe_code,
    reason = "hidden directory attributes require native handle queries"
)]
fn apply(file: &std::fs::File) -> io::Result<()> {
    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::zeroed();
    // SAFETY: the live handle and correctly sized output buffer survive the query.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a successful query initializes FILE_BASIC_INFO.
    let mut info = unsafe { info.assume_init() };
    if info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "retention container is not a directory",
        ));
    }
    info.FileAttributes = (info.FileAttributes & !FILE_ATTRIBUTE_NORMAL)
        | FILE_ATTRIBUTE_HIDDEN
        | FILE_ATTRIBUTE_SYSTEM;
    // SAFETY: the live handle and initialized input buffer survive the update.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw const info).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) struct Descriptor(pub(super) PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    #[allow(unsafe_code, reason = "the descriptor is a LocalAlloc allocation")]
    fn drop(&mut self) {
        // SAFETY: this guard owns the LocalAlloc allocation, or NULL.
        unsafe { LocalFree(self.0) };
    }
}

#[allow(
    unsafe_code,
    reason = "protected DACLs require Win32 security descriptors"
)]
fn protect(file: &std::fs::File) -> io::Result<()> {
    let descriptor = descriptor(PRIVATE_DACL)?;
    let dacl = dacl(&descriptor)?;
    // SAFETY: the live handle has WRITE_DAC and dacl points into the guarded descriptor.
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    verify_private(file, dacl)
}

#[allow(unsafe_code, reason = "SDDL conversion allocates a native descriptor")]
fn descriptor(sddl: &str) -> io::Result<Descriptor> {
    let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = Descriptor(ptr::null_mut());
    // SAFETY: sddl is NUL-terminated; the guard owns the allocation written to descriptor.0.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor.0,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(descriptor)
}

#[allow(unsafe_code, reason = "DACLs point into native security descriptors")]
fn dacl(descriptor: &Descriptor) -> io::Result<*mut ACL> {
    let (mut present, mut defaulted) = (0, 0);
    let mut dacl: *mut ACL = ptr::null_mut();
    // SAFETY: the descriptor is valid while the guard lives; the outputs are writable locals.
    let ok = unsafe {
        GetSecurityDescriptorDacl(
            descriptor.0,
            &raw mut present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    };
    if ok == 0 || present == 0 || dacl.is_null() {
        return Err(io::Error::other(
            "vault DACL is missing from its descriptor",
        ));
    }
    Ok(dacl)
}

#[allow(
    unsafe_code,
    reason = "DACL verification requires native handle queries"
)]
fn verify_private(file: &std::fs::File, expected: *const ACL) -> io::Result<()> {
    let mut found = Descriptor(ptr::null_mut());
    let mut dacl: *mut ACL = ptr::null_mut();
    // SAFETY: the live handle has READ_CONTROL; the guard owns the returned descriptor and DACL.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut dacl,
            ptr::null_mut(),
            &raw mut found.0,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    let (mut control, mut revision) = (0u16, 0u32);
    // SAFETY: a successful query returned a descriptor owned by found.
    let ok = unsafe { GetSecurityDescriptorControl(found.0, &raw mut control, &raw mut revision) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if control & SE_DACL_PROTECTED == 0 || !same_acl(dacl, expected)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "vault DACL is not private",
        ));
    }
    Ok(())
}

#[allow(
    unsafe_code,
    reason = "validated ACLs contain variable length native ACEs"
)]
fn same_acl(found: *const ACL, expected: *const ACL) -> io::Result<bool> {
    if found.is_null() || expected.is_null() {
        return Ok(false);
    }
    // SAFETY: both non-null pointers live in descriptors owned by the caller.
    let valid = unsafe { IsValidAcl(found) != 0 && IsValidAcl(expected) != 0 };
    if !valid {
        return Ok(false);
    }
    // SAFETY: both ACL headers passed IsValidAcl.
    let count = unsafe { (*expected).AceCount };
    // SAFETY: found passed IsValidAcl.
    if unsafe { (*found).AceCount } != count {
        return Ok(false);
    }
    for index in 0..u32::from(count) {
        let (mut left, mut right) = (ptr::null_mut(), ptr::null_mut());
        // SAFETY: the ACLs are valid and index is below both ACE counts.
        if unsafe {
            GetAce(found, index, &raw mut left) == 0 || GetAce(expected, index, &raw mut right) == 0
        } {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetAce returned ACEs in validated ACLs; AceSize bounds each ACE's bytes.
        let equal = unsafe {
            let left_size = usize::from((*left.cast::<ACE_HEADER>()).AceSize);
            let right_size = usize::from((*right.cast::<ACE_HEADER>()).AceSize);
            std::slice::from_raw_parts(left.cast::<u8>(), left_size)
                == std::slice::from_raw_parts(right.cast::<u8>(), right_size)
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[allow(unsafe_code, reason = "the DACL is read back through Win32")]
mod tests {
    use super::*;
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Security::Authorization::GetSecurityInfo;
    use windows_sys::Win32::Security::{GetSecurityDescriptorControl, SE_DACL_PROTECTED};

    #[test]
    fn hidden_attribute_is_set_only_on_the_confirmed_directory() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("container");
        std::fs::create_dir(&path).unwrap();
        let identity = crate::local::identity::read(&path).unwrap().unwrap();
        set(&path, identity).unwrap();
        let attributes = std::fs::metadata(&path).unwrap().file_attributes();
        assert_ne!(attributes & FILE_ATTRIBUTE_HIDDEN, 0);
        assert_ne!(attributes & FILE_ATTRIBUTE_SYSTEM, 0);
        let original = root.path().join("owned");
        std::fs::rename(&path, &original).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(set(&path, identity).is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN,
            0
        );
    }

    #[test]
    fn a_vault_gets_a_protected_dacl_for_owner_system_and_administrators() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("vault");
        std::fs::create_dir(&path).unwrap();
        set(&path, crate::local::identity::read(&path).unwrap().unwrap()).unwrap();
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&path)
            .unwrap();
        let mut found = Descriptor(ptr::null_mut());
        let mut dacl: *mut ACL = ptr::null_mut();
        // SAFETY: the live handle has READ_CONTROL; the guard owns the returned descriptor.
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &raw mut dacl,
                ptr::null_mut(),
                &raw mut found.0,
            )
        };
        assert_eq!(status, 0);
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the descriptor is valid while the guard lives.
        let ok =
            unsafe { GetSecurityDescriptorControl(found.0, &raw mut control, &raw mut revision) };
        assert_ne!(ok, 0);
        assert_ne!(
            control & SE_DACL_PROTECTED,
            0,
            "inherited ACEs must not reach the vault"
        );
        // SAFETY: GetSecurityInfo returned a DACL inside the guarded descriptor.
        let aces = unsafe { (*dacl).AceCount };
        assert_eq!(aces, 3, "owner, SYSTEM and Administrators only");
    }

    #[test]
    fn dacl_readback_rejects_broad_permissions_even_with_three_entries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("vault");
        std::fs::create_dir(&path).unwrap();
        let file = OpenOptions::new()
            .access_mode(READ_CONTROL | WRITE_DAC)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&path)
            .unwrap();
        let private = descriptor(PRIVATE_DACL).unwrap();
        let expected = dacl(&private).unwrap();
        let broad = descriptor("D:P(A;OICI;FA;;;WD)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)").unwrap();
        let broad_acl = dacl(&broad).unwrap();
        // SAFETY: the handle has WRITE_DAC; the ACL lives in the guarded descriptor.
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                broad_acl,
                ptr::null(),
            )
        };
        assert_eq!(status, 0);
        assert!(verify_private(&file, expected).is_err());
        protect(&file).unwrap();
        verify_private(&file, expected).unwrap();
    }
}

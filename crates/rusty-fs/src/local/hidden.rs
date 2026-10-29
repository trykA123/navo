use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
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
        result => warn_failed(path, "hide", result),
    }
    if super::win_owner::keeps_acls(&file).unwrap_or(false) {
        warn_failed(path, "protect", protect(&file));
    }
    Ok(())
}

fn warn_failed(path: &Path, step: &str, result: io::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(path = %path.display(), step, %error, "retention container step failed");
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
        // SAFETY: this guard owns the LocalAlloc allocation from the SDDL conversion, or NULL.
        unsafe { LocalFree(self.0) };
    }
}

#[allow(
    unsafe_code,
    reason = "protected DACLs require Win32 security descriptors"
)]
fn protect(file: &std::fs::File) -> io::Result<()> {
    let sddl: Vec<u16> = PRIVATE_DACL.encode_utf16().chain(Some(0)).collect();
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
    Ok(())
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
}

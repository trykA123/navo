use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_WRITE_ATTRIBUTES, FileBasicInfo, GetFileInformationByHandleEx, SetFileInformationByHandle,
};

pub(super) fn set(path: &Path, expected: crate::FileIdentity) -> io::Result<()> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if super::identity::from_file(&file)? != Some(expected) {
        return Err(io::Error::other("retention container identity changed"));
    }
    apply(&file)
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
        return Err(io::Error::other("retention container is not a directory"));
    }
    info.FileAttributes = (info.FileAttributes & !FILE_ATTRIBUTE_NORMAL) | FILE_ATTRIBUTE_HIDDEN;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::MetadataExt;

    #[test]
    fn hidden_attribute_is_set_only_on_the_confirmed_directory() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("container");
        std::fs::create_dir(&path).unwrap();
        let identity = crate::local::identity::read(&path).unwrap().unwrap();
        set(&path, identity).unwrap();
        assert_ne!(
            std::fs::metadata(&path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN,
            0
        );
        let original = root.path().join("owned");
        std::fs::rename(&path, &original).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(set(&path, identity).is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN,
            0
        );
    }
}

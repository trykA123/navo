//! Exclusive change.lock file guard with ACL, owner and link validation.

#![allow(unsafe_code, reason = "exclusive change files require Win32 FFI")]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, IsWellKnownSid, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, SetKernelObjectSecurity,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_DELETE_ON_CLOSE,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_TYPE_DISK, GetFileInformationByHandle, GetFileType,
    OPEN_ALWAYS, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles;

use crate::directory::{ProtectedDirectory, known_folder, protected_acl};
use crate::elevation::{LocalAllocation, wide};

pub(crate) struct FileLock {
    _handle: OwnedHandle,
    _directory: ProtectedDirectory,
}

impl FileLock {
    pub(super) fn acquire(wait: Duration) -> io::Result<Option<Self>> {
        let directory = known_folder(&FOLDERID_ProgramFiles)?.join("Navo");
        let file = directory.join("change.lock");
        let retry_started = Instant::now();
        let handle = super::acquire_file(
            wait,
            || {
                let guard = ProtectedDirectory::create_for_lock(&directory)?;
                open(&file).map(|handle| (guard, handle))
            },
            super::retryable_lock_error,
            move || retry_started.elapsed(),
            std::thread::sleep,
        )?;
        Ok(handle.map(|(guard, handle)| Self {
            _handle: handle,
            _directory: guard,
        }))
    }
}

fn open(path: &Path) -> io::Result<OwnedHandle> {
    let descriptor = descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: path is terminated and pinned by the directory guard; share mode zero is exclusive.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL | WRITE_DAC | DELETE,
            0,
            &attributes,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_DELETE_ON_CLOSE,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW transferred a valid uniquely owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    validate(&handle, &descriptor)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    Ok(handle)
}

fn descriptor() -> io::Result<LocalAllocation> {
    let sddl = wide("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)");
    let mut descriptor = LocalAllocation(ptr::null_mut());
    // SAFETY: sddl is terminated; descriptor owns the LocalAlloc output.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor.0,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(descriptor)
}

fn validate(handle: &OwnedHandle, expected: &LocalAllocation) -> io::Result<()> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is live and information is writable storage.
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut information) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle stays live through this type query.
    if unsafe { GetFileType(handle.as_raw_handle()) } != FILE_TYPE_DISK
        || information.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY)
            != 0
        || information.nNumberOfLinks != 1
    {
        return Err(denied("change.lock must be a regular file with one link"));
    }
    let actual = security(handle)?;
    if protected_acl(&actual) == protected_acl(expected) {
        return Ok(());
    }
    // SAFETY: only the DACL is changed on the verified, exclusively held file.
    if unsafe {
        SetKernelObjectSecurity(
            handle.as_raw_handle(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            expected.0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if protected_acl(&security(handle)?) != protected_acl(expected) {
        return Err(denied("change.lock protected DACL failed verification"));
    }
    Ok(())
}

fn security(handle: &OwnedHandle) -> io::Result<LocalAllocation> {
    let mut owner = ptr::null_mut();
    let mut actual = LocalAllocation(ptr::null_mut());
    // SAFETY: outputs are writable; actual owns the descriptor containing owner.
    let code = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut actual.0,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    // SAFETY: a non-NULL owner points into the live security descriptor.
    let trusted = !owner.is_null()
        && unsafe {
            IsWellKnownSid(owner, WinLocalSystemSid) != 0
                || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0
        };
    if !trusted {
        return Err(denied("change.lock owner must be SYSTEM or Administrators"));
    }
    Ok(actual)
}

fn denied(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason.to_owned())
}

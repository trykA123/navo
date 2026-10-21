#![allow(
    unsafe_code,
    reason = "secure service directories require Win32 security descriptors"
)]

use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::{ffi::OsString, ptr};

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    IsWellKnownSid, OWNER_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

use super::directory_policy::{Found, apply, directory_action};
use super::elevation::{LocalAllocation, wide};

#[derive(Clone, Copy)]
pub(super) enum Policy {
    Data,
    Executable,
}

pub(super) struct ProtectedDirectory {
    guards: Vec<OwnedHandle>,
}

impl ProtectedDirectory {
    pub fn open(paths: &[&Path], policy: Policy, install: bool) -> io::Result<Self> {
        let descriptor = descriptor(policy)?;
        if install {
            let _ = super::ownership::enable_privileges();
        }
        let mut guards = Vec::new();
        for path in paths {
            let guard = open_verified(path, &descriptor, policy, install).map_err(|error| {
                io::Error::new(error.kind(), format!("{}: {error}", path.display()))
            })?;
            guards.push(guard);
        }
        Ok(Self { guards })
    }

    pub fn remove_last(&mut self, path: &Path) -> io::Result<()> {
        drop(self.guards.pop());
        match std::fs::remove_dir(path) {
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
            result => result,
        }
    }
}

fn open_verified(
    path: &Path,
    descriptor: &LocalAllocation,
    policy: Policy,
    install: bool,
) -> io::Result<OwnedHandle> {
    if install {
        create_directory(path, descriptor)?;
    }
    let guard = open_directory(path, install)?;
    let kind = found(&guard)?;
    let trusted = kind == Found::Directory && is_trusted(&guard, descriptor, policy)?;
    let action = directory_action(kind, trusted, install);
    let repaired = apply(action, path, || {
        super::ownership::take_ownership(&guard, descriptor)
    })?;
    if repaired && !is_trusted(&guard, descriptor, policy)? {
        return Err(denied(
            "directory owner or protected ACL failed verification after repair",
        ));
    }
    Ok(guard)
}

struct FolderAllocation(*mut std::ffi::c_void);

impl Drop for FolderAllocation {
    fn drop(&mut self) {
        // SAFETY: this guard owns the known-folder allocation, including on API failure, or NULL.
        unsafe { CoTaskMemFree(self.0) };
    }
}

pub(super) fn known_folder(folder: &windows_sys::core::GUID) -> io::Result<PathBuf> {
    let mut raw = ptr::null_mut();
    // SAFETY: the folder GUID is valid and raw is writable pointer storage; the returned allocation is freed below.
    let result = unsafe { SHGetKnownFolderPath(folder, 0, ptr::null_mut(), &mut raw) };
    let _allocation = FolderAllocation(raw.cast());
    if result < 0 || raw.is_null() {
        return Err(io::Error::other(format!(
            "cannot locate known folder: {result:#x}"
        )));
    }
    // SAFETY: SHGetKnownFolderPath returned a NUL-terminated UTF-16 allocation.
    let result = unsafe {
        let mut length = 0;
        while *raw.add(length) != 0 {
            length += 1;
        }
        PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, length)))
    };
    Ok(result)
}

fn descriptor(policy: Policy) -> io::Result<LocalAllocation> {
    let sddl = wide(match policy {
        Policy::Data => {
            "O:S-1-5-32-544G:S-1-5-32-544D:P(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)"
        }
        Policy::Executable => concat!(
            "O:S-1-5-32-544G:S-1-5-32-544D:P(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)",
            "(A;OICI;FA;;;S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464)",
            "(A;OICI;FRFX;;;S-1-5-32-545)"
        ),
    });
    let mut descriptor = LocalAllocation(ptr::null_mut());
    // SAFETY: sddl is NUL-terminated; descriptor owns the LocalAlloc output.
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

fn create_directory(path: &Path, descriptor: &LocalAllocation) -> io::Result<()> {
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // SAFETY: path and the security descriptor stay live through the synchronous creation.
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error);
        }
    }
    Ok(())
}

fn open_directory(path: &Path, install: bool) -> io::Result<OwnedHandle> {
    let repair = if install { WRITE_OWNER | WRITE_DAC } else { 0 };
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: path is NUL-terminated; OPEN_REPARSE_POINT prevents following the final junction; omitting SHARE_DELETE pins the directory.
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            READ_CONTROL | FILE_READ_ATTRIBUTES | repair,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW transferred a valid uniquely owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn found(handle: &OwnedHandle) -> io::Result<Found> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: handle is live and information is writable storage of the declared structure type.
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut information) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let attributes = information.dwFileAttributes;
    Ok(if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Found::ReparsePoint
    } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        Found::Directory
    } else {
        Found::Other
    })
}

fn is_trusted(
    handle: &OwnedHandle,
    expected: &LocalAllocation,
    policy: Policy,
) -> io::Result<bool> {
    let mut owner = ptr::null_mut();
    let mut actual = LocalAllocation(ptr::null_mut());
    // SAFETY: all outputs are writable; actual owns the returned security descriptor.
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
    // SAFETY: owner refers to the live actual security descriptor.
    let trusted_owner = !owner.is_null()
        && unsafe {
            IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0
                || (matches!(policy, Policy::Data) && IsWellKnownSid(owner, WinLocalSystemSid) != 0)
        };
    let expected =
        protected_acl(expected).ok_or_else(|| denied("expected ACL is not protected"))?;
    Ok(trusted_owner && protected_acl(&actual) == Some(expected))
}

fn protected_acl(descriptor: &LocalAllocation) -> Option<&[u8]> {
    let (mut control, mut revision, mut present, mut defaulted) = (0, 0, 0, 0);
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: descriptor is live and all outputs point to correctly typed writable storage.
    let ok = unsafe {
        GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) != 0
            && GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) != 0
    };
    if !ok || control & SE_DACL_PROTECTED == 0 || present == 0 || acl.is_null() {
        return None;
    }
    // SAFETY: the API returned a valid ACL within descriptor; AclSize bounds its initialized bytes.
    Some(unsafe { std::slice::from_raw_parts(acl.cast(), (*acl).AclSize as usize) })
}

fn denied(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason.to_owned())
}

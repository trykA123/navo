//! Windows parent ACL probe: refuses an existing folder another user can change.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "reading owners, DACLs and ACEs requires Win32 FFI"
)]

use std::{io, path::Path, ptr};

use windows_sys::Win32::{
    Foundation::{GENERIC_ALL, GENERIC_WRITE},
    Security::{
        ACCESS_ALLOWED_ACE, ACL,
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, INHERIT_ONLY_ACE, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID,
    },
    Storage::FileSystem::{DELETE, FILE_DELETE_CHILD, WRITE_DAC, WRITE_OWNER},
};

use crate::payload::platform::security;
use crate::placement::Chain;

/// The well-known SID text for LocalSystem.
const SYSTEM: &str = "S-1-5-18";
/// The well-known SID text for BUILTIN\Administrators.
const ADMINISTRATORS: &str = "S-1-5-32-544";
/// The well-known SID text for NT SERVICE\TrustedInstaller.
const TRUSTED_INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
/// The placeholder that grants each object's owner rights; Navo's owner is this user.
const CREATOR_OWNER: &str = "S-1-3-0";
/// The placeholder that grants each object's owner rights without inherited control.
const OWNER_RIGHTS: &str = "S-1-3-4";

/// Rights that let another SID change the folder itself.
const CHANGES_FOLDER: u32 = DELETE | WRITE_DAC | WRITE_OWNER | GENERIC_ALL | GENERIC_WRITE;

/// Whether an existing folder above the candidate, except the drive root, is open to others.
pub(super) fn parent_open(chain: &Chain) -> io::Result<bool> {
    let user = security::user_sid()?;
    for link in chain.existing.iter().skip(1) {
        if folder_open(&link.path, &user)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads one folder's owner and DACL; an error is returned so the caller refuses.
fn folder_open(path: &Path, user: &str) -> io::Result<bool> {
    let wide = wide_z(path);
    let mut owner: PSID = ptr::null_mut();
    let mut acl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the path is NUL-terminated; owner, acl and descriptor are writable outputs.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    let _allocation = security::Local(descriptor);
    if !trusted(&security::sid_text(owner)?, user) {
        return Ok(true);
    }
    if acl.is_null() {
        return Ok(true);
    }
    // SAFETY: the successful query returned a live ACL allocation.
    let count = u32::from(unsafe { (*acl).AceCount });
    for index in 0..count {
        if ace_open(acl, index, user)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether one allow ACE gives another SID rights over this folder or its children.
fn ace_open(acl: *const ACL, index: u32, user: &str) -> io::Result<bool> {
    let mut raw = ptr::null_mut();
    // SAFETY: acl is live and index is within its verified AceCount.
    if unsafe { GetAce(acl, index, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetAce returned an ACE whose header is readable before its type is interpreted.
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    if ace.Header.AceType != 0 {
        return Ok(false);
    }
    let flags = u32::from(ace.Header.AceFlags);
    let applies = flags & INHERIT_ONLY_ACE == 0;
    let changes_folder = applies && ace.Mask & CHANGES_FOLDER != 0;
    let deletes_child = ace.Mask & FILE_DELETE_CHILD != 0;
    if !changes_folder && !deletes_child {
        return Ok(false);
    }
    // SAFETY: an access-allowed ACE stores its SID at SidStart.
    let sid = security::sid_text(ptr::from_ref(&ace.SidStart).cast_mut().cast())?;
    Ok(!trusted(&sid, user))
}

/// Whether a SID is one that may change Navo's install chain.
fn trusted(sid: &str, user: &str) -> bool {
    sid == user
        || sid == SYSTEM
        || sid == ADMINISTRATORS
        || sid == TRUSTED_INSTALLER
        || sid == CREATOR_OWNER
        || sid == OWNER_RIGHTS
}

/// A NUL-terminated UTF-16 path for the Win32 call.
fn wide_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SE_FILE_OBJECT,
            SetNamedSecurityInfoW,
        },
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    use super::*;
    use crate::placement::{FileId, Link};

    /// Builds a chain whose only checked parent is `path`.
    fn chain_for(path: &Path) -> Chain {
        let link = |path: &Path| Link {
            path: path.to_path_buf(),
            name: path.file_name().unwrap_or_default().into(),
            id: FileId { volume: 0, id: 0 },
            reparse: false,
        };
        Chain {
            existing: vec![link(Path::new("C:\\")), link(path)],
            missing: Vec::new(),
        }
    }

    /// Replaces one folder's DACL with `sddl`.
    fn set_dacl(path: &Path, sddl: &str) -> io::Result<()> {
        let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = ptr::null_mut();
        // SAFETY: SDDL is NUL-terminated and the descriptor output is writable.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _allocation = security::Local(descriptor);
        let mut present = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        let mut defaulted = 0;
        // SAFETY: the descriptor is live and the DACL outputs are writable.
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the path is NUL-terminated and the DACL stays live for the call.
        let status = unsafe {
            SetNamedSecurityInfoW(
                wide_z(path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status.cast_signed()));
        }
        Ok(())
    }

    /// A user/SY/BA DACL is closed; adding an Authenticated Users allow ACE opens it.
    #[test]
    fn an_authenticated_users_ace_makes_the_parent_open() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let user = security::user_sid()?;
        let closed = format!("D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        set_dacl(temp.path(), &closed)?;
        assert!(!parent_open(&chain_for(temp.path()))?);
        set_dacl(temp.path(), &format!("{closed}(A;;FA;;;AU)"))?;
        assert!(parent_open(&chain_for(temp.path()))?);
        Ok(())
    }
}

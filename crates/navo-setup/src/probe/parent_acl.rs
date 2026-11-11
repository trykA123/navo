//! Windows parent ACL probe: refuses an existing folder another user can change.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "reading owners, DACLs and ACEs requires Win32 FFI"
)]

use std::{io, path::Path, ptr};

use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL,
    Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
    DACL_SECURITY_INFORMATION, GetAce, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};

use crate::parent_acl_rule::ace_opens;
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

/// Whether an existing folder or the drive root is open to another user.
pub(super) fn parent_open(chain: &Chain) -> io::Result<bool> {
    let user = security::user_sid()?;
    for (index, link) in chain.existing.iter().enumerate() {
        if folder_open(&link.path, &user, index == 0)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads one folder's owner and DACL; the root uses the narrower rule and ignores its owner.
fn folder_open(path: &Path, user: &str, is_root: bool) -> io::Result<bool> {
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
        return if is_root {
            Ok(true)
        } else {
            Err(io::Error::from_raw_os_error(status.cast_signed()))
        };
    }
    let _allocation = security::Local(descriptor);
    if !is_root && !trusted(&security::sid_text(owner)?, user) {
        return Ok(true);
    }
    if acl.is_null() {
        return Ok(true);
    }
    // SAFETY: the successful query returned a live ACL allocation.
    let count = u32::from(unsafe { (*acl).AceCount });
    for index in 0..count {
        if ace_open(acl, index, user, is_root)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether one allow ACE gives another SID rights over this folder or the root.
fn ace_open(acl: *const ACL, index: u32, user: &str, is_root: bool) -> io::Result<bool> {
    let mut raw = ptr::null_mut();
    // SAFETY: acl is live and index is within its verified AceCount.
    if unsafe { GetAce(acl, index, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetAce returned an ACE whose header is readable before its type is interpreted.
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    let ace_type = ace.Header.AceType;
    let flags = u32::from(ace.Header.AceFlags);
    let trusted = if ace_type == 0 {
        // SAFETY: an access-allowed ACE stores its SID at this fixed offset from its start.
        let sid = unsafe {
            raw.cast::<u8>()
                .add(std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart))
                .cast()
        };
        trusted(&security::sid_text(sid)?, user)
    } else {
        false
    };
    Ok(ace_opens(ace_type, flags, ace.Mask, is_root, trusted))
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

    /// Replaces one folder's DACL with a null DACL, which grants everyone full access.
    fn set_null_dacl(path: &Path) -> io::Result<()> {
        // SAFETY: the path is NUL-terminated; a null DACL is the intended value.
        let status = unsafe {
            SetNamedSecurityInfoW(
                wide_z(path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
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
        assert!(!folder_open(temp.path(), &user, false)?);
        set_dacl(temp.path(), &format!("{closed}(A;;FA;;;AU)"))?;
        assert!(folder_open(temp.path(), &user, false)?);
        Ok(())
    }

    /// The root rule opens on a null DACL, FILE_DELETE_CHILD, WRITE_DAC or GENERIC_ALL only.
    #[test]
    fn the_root_rule_ignores_delete_and_generic_write() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let user = security::user_sid()?;
        let closed = format!("D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        set_dacl(temp.path(), &closed)?;
        assert!(!folder_open(temp.path(), &user, true)?);
        set_null_dacl(temp.path())?;
        assert!(folder_open(temp.path(), &user, true)?);
        for (right, open) in [("GA", true), ("WD", true), ("SD", false), ("GW", false)] {
            set_dacl(temp.path(), &format!("{closed}(A;;{right};;;AU)"))?;
            assert_eq!(folder_open(temp.path(), &user, true)?, open, "{right}");
        }
        Ok(())
    }
}

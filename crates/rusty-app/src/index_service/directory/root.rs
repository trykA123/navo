use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Win32::Foundation::{GENERIC_ALL, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
    INHERIT_ONLY_ACE, IsWellKnownSid, OWNER_SECURITY_INFORMATION, WinBuiltinAdministratorsSid,
    WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_DELETE_CHILD, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, WRITE_DAC,
    WRITE_OWNER,
};

use super::super::elevation::{LocalAllocation, wide};

const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;

pub(super) fn is_trusted(handle: &OwnedHandle) -> io::Result<bool> {
    let mut installer = LocalAllocation(ptr::null_mut());
    let sid = wide("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464");
    // SAFETY: sid is terminated and installer owns the returned LocalAlloc allocation.
    if unsafe { ConvertStringSidToSidW(sid.as_ptr(), &mut installer.0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let (mut owner, mut acl) = (ptr::null_mut(), ptr::null_mut::<ACL>());
    let mut descriptor = LocalAllocation(ptr::null_mut());
    // SAFETY: the handle is pinned; all outputs are writable and descriptor owns the result.
    let code = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor.0,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    // SAFETY: owner and acl belong to the live security descriptor returned above.
    unsafe { check_acl(owner, acl, installer.0) }
}

unsafe fn privileged(sid: *mut c_void, installer: *mut c_void) -> bool {
    // SAFETY: the caller supplies live SIDs from Windows security APIs.
    !sid.is_null()
        && unsafe {
            IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
                || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || EqualSid(sid, installer) != 0
        }
}

unsafe fn check_acl(owner: *mut c_void, acl: *mut ACL, installer: *mut c_void) -> io::Result<bool> {
    // SAFETY: the caller keeps the security descriptor and installer SID alive.
    if acl.is_null() || !unsafe { privileged(owner, installer) } {
        return Ok(false);
    }
    // SAFETY: acl points to a valid ACL returned by GetSecurityInfo.
    for index in 0..unsafe { (*acl).AceCount } {
        let mut ace = ptr::null_mut();
        // SAFETY: index is within the live ACL and ace is writable pointer storage.
        if unsafe { GetAce(acl, u32::from(index), &mut ace) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetAce returned a valid ACE containing its header.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceFlags & INHERIT_ONLY_ACE as u8 != 0 || header.AceType == ACCESS_DENIED_ACE_TYPE
        {
            continue;
        }
        if header.AceType != ACCESS_ALLOWED_ACE_TYPE {
            return Ok(false);
        }
        // SAFETY: type 0 is ACCESS_ALLOWED_ACE; its SID is inline at SidStart.
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = ptr::addr_of!(allowed.SidStart).cast_mut().cast();
        let mutations = DELETE
            | FILE_DELETE_CHILD
            | WRITE_DAC
            | WRITE_OWNER
            | GENERIC_ALL
            | GENERIC_WRITE
            | FILE_WRITE_DATA
            | FILE_WRITE_EA
            | FILE_WRITE_ATTRIBUTES;
        // SAFETY: sid is within the live ACE returned by GetAce.
        if allowed.Mask & mutations != 0 && !unsafe { privileged(sid, installer) } {
            return Ok(false);
        }
    }
    Ok(true)
}

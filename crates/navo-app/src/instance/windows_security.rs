use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::{owned, wide};

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: the conversion API allocated this pointer with LocalAlloc; this owner frees it once.
        let _ = unsafe { LocalFree(self.0) };
    }
}

pub(super) struct PipeSecurity(LocalAllocation);

impl PipeSecurity {
    pub(super) fn current_user() -> io::Result<Self> {
        let sddl = wide(&format!("D:P(A;;GA;;;{})", current_user_sid()?));
        let mut descriptor = null_mut();
        // SAFETY: sddl is NUL-terminated and descriptor points to writable pointer storage.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(LocalAllocation(descriptor)))
    }

    pub(super) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: 0,
        }
    }
}

fn current_user_sid() -> io::Result<String> {
    let mut raw = null_mut();
    // SAFETY: the pseudo-handle is current, TOKEN_QUERY permits reading, and raw is writable handle storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(raw)?;
    let user = token_user(&token)?;
    // SAFETY: GetTokenInformation initialized the aligned buffer with a TOKEN_USER and its SID.
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = null_mut();
    // SAFETY: the SID remains in the live user buffer; text is writable pointer storage.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(text.cast());
    // SAFETY: ConvertSidToStringSidW returned a live, NUL-terminated UTF-16 allocation.
    let text = unsafe {
        let mut length = 0;
        while *text.add(length) != 0 {
            length += 1;
        }
        std::slice::from_raw_parts(text, length)
    };
    Ok(String::from_utf16_lossy(text))
}

fn token_user(token: &OwnedHandle) -> io::Result<Vec<usize>> {
    let mut length = 0;
    // SAFETY: a null, zero-sized output queries the required size; length is writable.
    let ok = unsafe {
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length)
    };
    let error = io::Error::last_os_error();
    if ok == 0 && error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error);
    }
    if (length as usize) < size_of::<TOKEN_USER>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing token user",
        ));
    }
    let mut user = vec![0_usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: the buffer is pointer-aligned and has at least length writable bytes for TOKEN_USER and its SID.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(user)
}

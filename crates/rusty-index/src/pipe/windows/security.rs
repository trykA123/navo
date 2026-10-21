use std::ptr;

use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
            SE_KERNEL_OBJECT,
        },
        IsWellKnownSid, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, WinLocalSystemSid,
    },
    Storage::FileSystem::{FILE_CREATE_PIPE_INSTANCE, FILE_GENERIC_READ, FILE_GENERIC_WRITE},
};

use super::super::PipeError;
use super::{io_error, wide};

pub(super) fn verify_owner(handle: HANDLE) -> Result<(), PipeError> {
    let mut owner = ptr::null_mut();
    let mut descriptor = SecurityDescriptor(ptr::null_mut());
    // SAFETY: the handle is live, all outputs are valid; descriptor owns the returned LocalAlloc allocation.
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor.0,
        )
    };
    if result != 0 || owner.is_null() {
        return Err(PipeError::NotAvailable);
    }
    // SAFETY: owner points into the live descriptor returned by GetSecurityInfo.
    if unsafe { IsWellKnownSid(owner, WinLocalSystemSid) } == 0 {
        return Err(PipeError::NotAvailable);
    }
    Ok(())
}

pub(super) struct SecurityDescriptor(*mut std::ffi::c_void);

impl SecurityDescriptor {
    pub(super) fn new(sid: &str) -> Result<Self, PipeError> {
        Self::for_owner(sid, "O:SY")
    }

    #[cfg(test)]
    pub(super) fn new_test(sid: &str) -> Result<Self, PipeError> {
        Self::for_owner(sid, "")
    }

    fn for_owner(sid: &str, owner: &str) -> Result<Self, PipeError> {
        if !valid_user_sid(sid) {
            return Err(PipeError::InvalidSid);
        }
        let access = FILE_GENERIC_READ | (FILE_GENERIC_WRITE & !FILE_CREATE_PIPE_INSTANCE);
        let sddl = wide(&format!(
            "{owner}D:P(A;;FA;;;S-1-5-18)(A;;FA;;;S-1-5-32-544)(A;;0x{access:08x};;;{sid})"
        ));
        let mut descriptor = Self(ptr::null_mut());
        // SAFETY: sddl is NUL-terminated; the output guard owns and frees the converted descriptor.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor.0,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io_error());
        }
        Ok(descriptor)
    }

    pub(super) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

fn valid_user_sid(sid: &str) -> bool {
    if sid.len() > 184 {
        return false;
    }
    let Some(parts) = sid.strip_prefix("S-1-5-21-") else {
        return false;
    };
    let mut parts = parts.split('-');
    let mut rid = 0;
    for _ in 0..4 {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        let Ok(value) = part.parse::<u32>() else {
            return false;
        };
        rid = value;
    }
    parts.next().is_none() && ((500..=504).contains(&rid) || rid >= 1000)
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns its non-NULL LocalAlloc allocation; NULL is also accepted.
        unsafe { LocalFree(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_account_user_sid_shapes_are_accepted() {
        for sid in [
            "S-1-5-18",
            "S-1-5-32-544",
            "S-1-5-21-1-2-3-512",
            "S-1-5-21-1-2-3",
            "S-1-5-21-1-2-3-1000-1",
            "S-1-5-21-1-2-3-1000)",
        ] {
            assert!(matches!(
                SecurityDescriptor::new(sid),
                Err(PipeError::InvalidSid)
            ));
        }
        assert!(valid_user_sid("S-1-5-21-1-2-3-1000"));
        assert!(valid_user_sid("S-1-5-21-1-2-3-500"));
    }
}

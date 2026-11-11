//! Windows security descriptor and random names for private installer work folders.

use std::{
    ffi::c_void,
    io,
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// An allocation returned by LocalAlloc-based security APIs.
pub(crate) struct Local(pub(crate) *mut c_void);
impl Drop for Local {
    /// Releases the owned Windows allocation once.
    fn drop(&mut self) {
        // SAFETY: the pointer is owned and was allocated by a LocalAlloc-based API.
        unsafe { LocalFree(self.0) };
    }
}

/// Returns the current process user's SID text, or the Windows query error.
pub(super) fn user_sid() -> io::Result<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: the pseudo-handle is valid and raw is a writable token output.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken returned an owned, valid handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut size = 0;
    // SAFETY: a null buffer with zero size queries the required allocation length.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut size,
        )
    };
    if size == 0 {
        return Err(io::Error::last_os_error());
    }
    let words = usize::try_from(size)
        .map_err(io::Error::other)?
        .div_ceil(size_of::<usize>());
    let mut buffer = vec![0usize; words];
    // SAFETY: the aligned buffer has at least size bytes, and token remains live.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful TokenUser output initializes TOKEN_USER at this aligned address.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_text(user.User.Sid)
}

/// Converts a valid Windows SID to text and releases the temporary string allocation.
pub(super) fn sid_text(sid: *mut c_void) -> io::Result<String> {
    let mut text = ptr::null_mut();
    // SAFETY: the caller supplies a live SID and text is writable output.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = Local(text.cast());
    let mut length = 0;
    // SAFETY: the successful conversion returns a live NUL-terminated UTF-16 string.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: length covers the initialized string, excluding its terminator.
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, length)
    }))
}

/// Builds the private work-folder SDDL: this user and SYSTEM, with no parent inheritance.
fn private_sddl(sid: &str) -> String {
    format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)")
}

/// Builds the protected setup-folder SDDL: this user, SYSTEM and Administrators, inherited.
pub(super) fn root_sddl(sid: &str) -> String {
    format!("{}(A;OICI;FA;;;BA)", private_sddl(sid))
}

/// Creates a protected DACL granting only this user and SYSTEM full access.
pub(super) fn private_descriptor() -> io::Result<Local> {
    let sid = user_sid()?;
    descriptor(&private_sddl(&sid))
}

/// Creates a protected folder DACL for this user, SYSTEM and Administrators with full access.
pub(crate) fn root_descriptor() -> io::Result<Local> {
    let sid = user_sid()?;
    descriptor(&root_sddl(&sid))
}

/// Converts one SDDL string to an allocated, self-relative security descriptor.
fn descriptor(sddl: &str) -> io::Result<Local> {
    let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: SDDL is NUL-terminated; output is allocated by the API on success.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(Local(descriptor))
}

/// Generates 128 random bits as 32 hex digits, failing if the system RNG fails.
pub(super) fn random_name() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    // SAFETY: bytes is writable for exactly 16 bytes; the system RNG needs no handle.
    let status = unsafe {
        BCryptGenRandom(
            ptr::null_mut(),
            bytes.as_mut_ptr(),
            16,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "BCryptGenRandom failed: {status:#x}"
        )));
    }
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

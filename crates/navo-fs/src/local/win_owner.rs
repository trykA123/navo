use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSID, TOKEN_INFORMATION_CLASS,
    TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER, TokenOwner, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    GetVolumeInformationByHandleW, READ_CONTROL,
};
use windows_sys::Win32::System::SystemServices::FILE_PERSISTENT_ACLS;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::hidden::Descriptor;

pub(super) fn of_path(path: &Path) -> io::Result<Option<bool>> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    of_file(&file)
}

pub(super) fn of_file(file: &File) -> io::Result<Option<bool>> {
    if !keeps_acls(file)? {
        return Ok(None);
    }
    let (_descriptor, owner) = owner(file)?;
    let token = Token::current()?;
    let user = token.query(TokenUser)?;
    let default_owner = token.query(TokenOwner)?;
    Ok(Some(
        same(owner, user_sid(&user)) || same(owner, default_owner_sid(&default_owner)),
    ))
}

#[allow(unsafe_code, reason = "the owner SID is read through Win32")]
fn owner(file: &File) -> io::Result<(Descriptor, PSID)> {
    let mut descriptor = Descriptor(ptr::null_mut());
    let mut owner: PSID = ptr::null_mut();
    // SAFETY: the live handle has READ_CONTROL; the guard owns the returned descriptor,
    // and the owner SID points into it.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut descriptor.0,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    if owner.is_null() {
        return Err(io::Error::other("vault has no owner"));
    }
    Ok((descriptor, owner))
}

#[allow(unsafe_code, reason = "token buffers hold Win32 structures")]
fn user_sid(buffer: &[u64]) -> PSID {
    // SAFETY: a successful TokenUser query starts the aligned buffer with TOKEN_USER.
    unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid }
}

#[allow(unsafe_code, reason = "token buffers hold Win32 structures")]
fn default_owner_sid(buffer: &[u64]) -> PSID {
    // SAFETY: a successful TokenOwner query starts the aligned buffer with TOKEN_OWNER.
    unsafe { (*buffer.as_ptr().cast::<TOKEN_OWNER>()).Owner }
}

#[allow(unsafe_code, reason = "SIDs are compared through Win32")]
fn same(left: PSID, right: PSID) -> bool {
    // SAFETY: both SIDs are valid while their descriptor and token buffers live.
    !left.is_null() && !right.is_null() && unsafe { EqualSid(left, right) } != 0
}

struct Token(HANDLE);

impl Token {
    #[allow(unsafe_code, reason = "the process token is opened through Win32")]
    fn current() -> io::Result<Self> {
        let mut token = Self(ptr::null_mut());
        // SAFETY: the process pseudo handle needs no closing; the guard owns the token handle.
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token.0) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(token)
    }

    #[allow(unsafe_code, reason = "token information is queried through Win32")]
    fn query(&self, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<u64>> {
        let mut needed = 0u32;
        // SAFETY: a NULL buffer with zero length only reports the size it needs.
        unsafe { GetTokenInformation(self.0, class, ptr::null_mut(), 0, &raw mut needed) };
        let words = usize::try_from(needed.div_ceil(8)).map_err(io::Error::other)?;
        let mut buffer = vec![0u64; words];
        // SAFETY: the aligned buffer is writable for at least `needed` bytes.
        let ok = unsafe {
            GetTokenInformation(
                self.0,
                class,
                buffer.as_mut_ptr().cast(),
                needed,
                &raw mut needed,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(buffer)
    }
}

impl Drop for Token {
    #[allow(unsafe_code, reason = "the token handle is closed through Win32")]
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard owns the open token handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[allow(unsafe_code, reason = "volume flags require a native handle query")]
pub(super) fn keeps_acls(file: &File) -> io::Result<bool> {
    let mut flags = 0u32;
    // SAFETY: the live handle survives the call; every unused buffer is NULL with zero length.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut flags,
            ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags & FILE_PERSISTENT_ACLS != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_this_process_created_is_owned_by_the_current_user() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".navo-vault");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(of_path(&path).unwrap(), Some(true));
    }

    #[test]
    fn a_system_owned_folder_is_not_owned_by_the_current_user() {
        let windows = std::env::var_os("SystemRoot").unwrap();
        let system = Path::new(&windows).join("System32");
        assert_eq!(of_path(&system).unwrap(), Some(false));
    }
}

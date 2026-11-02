#![allow(
    unsafe_code,
    reason = "Windows ownership and ACL queries require native APIs"
)]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    FreeInheritedFromArray, GetInheritanceSourceW, GetNamedSecurityInfoW, INHERITED_FROMW,
    SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GENERIC_MAPPING, GetAce,
    INHERITED_ACE, IsValidSid, LookupAccountSidW, MapGenericMask, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SID_NAME_USE,
};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    READ_CONTROL, SYNCHRONIZE,
};
use windows::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
use windows::core::{PCWSTR, PWSTR};

use crate::{AclRow, EntryKind, FileDetails};

const FILE_MAPPING: GENERIC_MAPPING = GENERIC_MAPPING {
    GenericRead: FILE_GENERIC_READ.0,
    GenericWrite: FILE_GENERIC_WRITE.0,
    GenericExecute: FILE_GENERIC_EXECUTE.0,
    GenericAll: FILE_ALL_ACCESS.0,
};

pub(super) fn read(path: &Path, details: &mut FileDetails) {
    let result = (|| {
        let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
        if name.contains(&0) {
            return Err(note("Security", io::ErrorKind::InvalidInput.into()));
        }
        name.push(0);
        let security = Security::read(&name).map_err(|error| note("Security", error))?;
        let owner = account_name(security.owner).map_err(|error| note("Owner", error))?;
        let (acl, inherited) = security.rows().map_err(|error| note("ACL", error))?;
        let inherited_from = if !inherited.is_empty() {
            Some(
                security
                    .inherited_from(&name, details.kind == EntryKind::Folder, &inherited)
                    .map_err(|error| note("ACL", error))?,
            )
        } else {
            None
        };
        Ok((owner, acl, inherited_from))
    })();
    match result {
        Ok((owner, acl, inherited_from)) => {
            details.owner = Some(owner);
            details.acl = acl;
            details.inherited_from = inherited_from;
        }
        Err(note) => details.security_note = Some(note),
    }
}

struct Security {
    descriptor: PSECURITY_DESCRIPTOR,
    owner: PSID,
    acl: *mut ACL,
}

impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: GetNamedSecurityInfoW allocated this descriptor with LocalAlloc.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
    }
}

impl Security {
    fn read(name: &[u16]) -> io::Result<Self> {
        let mut security = Self {
            descriptor: PSECURITY_DESCRIPTOR::default(),
            owner: PSID::default(),
            acl: ptr::null_mut(),
        };
        // SAFETY: name is terminated; outputs remain live and the descriptor owns its SID and ACL.
        let status = unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&raw mut security.owner),
                None,
                Some(&raw mut security.acl),
                None,
                &raw mut security.descriptor,
            )
        };
        status.ok().map_err(os_error)?;
        Ok(security)
    }

    fn rows(&self) -> io::Result<(Vec<AclRow>, Vec<usize>)> {
        if self.acl.is_null() {
            return Ok((Vec::new(), Vec::new()));
        }
        // SAFETY: acl points into the live security descriptor returned by Windows.
        let count = unsafe { (*self.acl).AceCount };
        let mut rows = Vec::new();
        let mut inherited = Vec::new();
        for index in 0..u32::from(count) {
            let mut ace = ptr::null_mut();
            // SAFETY: the ACL is live, index is in range, and ace is a writable output.
            unsafe { GetAce(self.acl, index, &raw mut ace) }.map_err(os_error)?;
            // SAFETY: GetAce returned an ACE with a valid common header.
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            if u32::from(header.AceType) != ACCESS_ALLOWED_ACE_TYPE {
                continue;
            }
            if usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            // SAFETY: the type and size identify an ACCESS_ALLOWED_ACE in the live ACL.
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let sid = PSID((&raw const allowed.SidStart).cast_mut().cast());
            rows.push(AclRow {
                who: account_name(sid)?,
                rights: rights(allowed.Mask).into(),
            });
            if u32::from(header.AceFlags) & INHERITED_ACE.0 != 0 {
                inherited.push(index as usize);
            }
        }
        Ok((rows, inherited))
    }

    fn inherited_from(
        &self,
        name: &[u16],
        is_folder: bool,
        inherited: &[usize],
    ) -> io::Result<String> {
        // SAFETY: this method is called only for a live, non-null ACL containing inherited ACEs.
        let count = unsafe { (*self.acl).AceCount };
        let mut sources = Inheritance(vec![INHERITED_FROMW::default(); usize::from(count)]);
        // SAFETY: all pointers are live; sources has one writable entry per ACE.
        let status = unsafe {
            GetInheritanceSourceW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                is_folder,
                None,
                self.acl,
                None,
                &FILE_MAPPING,
                sources.0.as_mut_ptr(),
            )
        };
        status.ok().map_err(os_error)?;
        let nearest = inherited
            .iter()
            .map(|&index| &sources.0[index])
            .filter(|source| source.GenerationGap > 0 && !source.AncestorName.is_null())
            .min_by_key(|source| source.GenerationGap)
            .ok_or(io::ErrorKind::NotFound)?;
        // SAFETY: Windows allocated a terminated ancestor name, owned by sources until return.
        unsafe { nearest.AncestorName.to_string() }.map_err(|_| io::ErrorKind::InvalidData.into())
    }
}

struct Inheritance(Vec<INHERITED_FROMW>);

impl Drop for Inheritance {
    fn drop(&mut self) {
        // SAFETY: these entries own only names allocated by GetInheritanceSourceW.
        let _ = unsafe { FreeInheritedFromArray(&self.0, None) };
    }
}

fn account_name(sid: PSID) -> io::Result<String> {
    // SAFETY: non-null SIDs come from the live security descriptor or one of its ACEs.
    if sid.0.is_null() || !unsafe { IsValidSid(sid) }.as_bool() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let (mut name_len, mut domain_len) = (0, 0);
    let mut usage = SID_NAME_USE::default();
    // SAFETY: the SID is valid; zero-sized outputs request the required buffer lengths.
    let query = unsafe {
        LookupAccountSidW(
            None,
            sid,
            None,
            &raw mut name_len,
            None,
            &raw mut domain_len,
            &raw mut usage,
        )
    };
    if let Err(error) = query
        && error.code() != windows::core::HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)
    {
        return Err(os_error(error));
    }
    let mut name = vec![0_u16; name_len as usize];
    let mut domain = vec![0_u16; domain_len as usize];
    // SAFETY: both buffers cover the requested lengths and the SID remains live.
    unsafe {
        LookupAccountSidW(
            None,
            sid,
            Some(PWSTR(name.as_mut_ptr())),
            &raw mut name_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &raw mut domain_len,
            &raw mut usage,
        )
    }
    .map_err(os_error)?;
    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    Ok(if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    })
}

fn rights(mut mask: u32) -> &'static str {
    // SAFETY: mask is writable and FILE_MAPPING defines file-specific generic rights.
    unsafe { MapGenericMask(&raw mut mask, &FILE_MAPPING) };
    let mask = mask & !SYNCHRONIZE.0;
    let read = FILE_GENERIC_READ.0 & !SYNCHRONIZE.0;
    let write = FILE_GENERIC_WRITE.0 & !SYNCHRONIZE.0;
    let execute = FILE_GENERIC_EXECUTE.0 & !SYNCHRONIZE.0;
    if mask == FILE_ALL_ACCESS.0 & !SYNCHRONIZE.0 {
        "Full control"
    } else if mask == read | write | execute | DELETE.0 {
        "Modify"
    } else if mask == read | execute {
        "Read and run"
    } else if mask == read {
        "Read"
    } else if mask == write || mask == write & !READ_CONTROL.0 {
        "Write"
    } else {
        "Special"
    }
}

fn os_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xffff)
}

fn note(subject: &str, error: io::Error) -> String {
    if error.kind() == io::ErrorKind::PermissionDenied {
        "Access denied".into()
    } else {
        format!("{subject} lookup failed: {error}")
    }
}

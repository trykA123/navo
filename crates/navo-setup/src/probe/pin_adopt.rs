//! Windows adopt for E8: holds the folders the elevated helper made and checks each one on the
//! handle it keeps (identity, owner and DACL, and an empty root); nothing is reopened by path.

#![allow(
    unsafe_code,
    reason = "security descriptors, ACEs and directory listings require Win32 FFI"
)]

use std::io;
use std::mem::{offset_of, size_of};
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce,
    GetSecurityDescriptorControl, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SE_DACL_PROTECTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ID_BOTH_DIR_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING, READ_CONTROL,
};

use super::{
    HOLD_ACCESS, HOLD_FLAGS, Held, Owned, Pin, PinError, ancestors_unchanged, read_id, wide_z,
};
use crate::adopt_rule::{self, Batch, Emptiness, NAME_OFFSET, ShapeError};
use crate::elevate::{AceView, admin_descriptor_ok};
use crate::payload::platform::security;
use crate::placement::{Chain, Link};
use crate::probe::chain::OrdinalNames;

#[cfg(test)]
#[path = "pin_adopt_tests.rs"]
mod tests;

// The pure listing parser hard-codes this offset so it can be tested off Windows.
const _: () = assert!(offset_of!(FILE_ID_BOTH_DIR_INFO, FileName) == NAME_OFFSET);
const _: () = assert!(offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength) == 60);

/// One listing batch; `.`, `..` and one more entry of any legal name length always fit.
const LISTING_WORDS: usize = 512;

/// The descriptor rule: owner SID text, DACL protected flag and ACEs in, accept or refuse out.
pub(super) type Accept<'rule> = dyn Fn(&str, bool, &[AceView]) -> bool + 'rule;

impl Pin {
    /// Adopts the folders the elevated helper created for `before`, as `after` resolves them now.
    ///
    /// Each folder must carry exactly `admin_root_sddl(user_sid)`. Adopt never deletes: on any
    /// failure it closes the handles it took and returns the first error.
    pub fn adopt(before: &Chain, after: &Chain, user_sid: &str) -> Result<Self, PinError> {
        adopt_checked(before, after, &|owner, protected, aces| {
            admin_descriptor_ok(owner, protected, aces, user_sid)
        })
    }
}

/// The adopt core with an injected descriptor rule, so unelevated tests can use their own owner.
///
/// Holds each new folder outermost first, then re-reads the existing ancestors' ids.
pub(super) fn adopt_checked(
    before: &Chain,
    after: &Chain,
    accept: &Accept<'_>,
) -> Result<Pin, PinError> {
    adopt_rule::adopt_shape(before, after, &OrdinalNames).map_err(|error| match error {
        ShapeError::InvalidChain => PinError::InvalidChain,
        ShapeError::Identity => PinError::Identity,
        ShapeError::Reparse => PinError::Reparse,
    })?;
    let new = after
        .existing
        .get(before.existing.len()..)
        .ok_or(PinError::Identity)?;
    let mut created = Vec::with_capacity(new.len());
    for (index, link) in new.iter().enumerate() {
        created.push(adopt_one(link, accept, index + 1 == new.len())?);
    }
    // The held folders block renames above them, so this id check cannot go stale.
    if !ancestors_unchanged(before) {
        return Err(PinError::Identity);
    }
    Ok(Pin {
        existing_root: None,
        created,
    })
}

/// Opens one new folder once and checks identity, descriptor and, for the root, emptiness.
fn adopt_one(link: &Link, accept: &Accept<'_>, is_root: bool) -> Result<Held, PinError> {
    let handle = open_adopt(&link.path)?;
    let (id, reparse, directory) = read_id(&handle)?;
    if reparse {
        return Err(PinError::Reparse);
    }
    if !directory {
        return Err(PinError::NotFolder);
    }
    if id != link.id {
        return Err(PinError::Identity);
    }
    if !descriptor_ok(&handle, accept)? {
        return Err(PinError::Descriptor);
    }
    if is_root && !root_empty(&handle)? {
        return Err(PinError::RootExists);
    }
    Ok(Held {
        path: link.path.clone(),
        id,
        handle: Some(handle),
    })
}

/// Opens a folder with the hold access plus `READ_CONTROL`; no delete sharing, so no rename.
fn open_adopt(path: &Path) -> io::Result<Owned> {
    let wide = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and the buffers are live for the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            HOLD_ACCESS | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            HOLD_FLAGS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Owned(handle))
}

/// Reads owner, DACL protection and ACEs through the handle; a missing owner or DACL refuses.
fn descriptor_ok(handle: &Owned, accept: &Accept<'_>) -> Result<bool, PinError> {
    let mut owner: PSID = ptr::null_mut();
    let mut acl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the handle is live; owner, acl and descriptor are writable outputs.
    let status = unsafe {
        GetSecurityInfo(
            handle.0,
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
        return Err(io::Error::from_raw_os_error(status.cast_signed()).into());
    }
    let _allocation = security::Local(descriptor);
    // A NULL DACL grants everyone everything; it never passes.
    if owner.is_null() || acl.is_null() {
        return Ok(false);
    }
    let (mut control, mut revision) = (0_u16, 0_u32);
    // SAFETY: the descriptor is the live allocation returned above; both outputs are writable.
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let owner = security::sid_text(owner)?;
    // SAFETY: the successful query returned a live ACL allocation.
    let count = u32::from(unsafe { (*acl).AceCount });
    let aces = (0..count)
        .map(|index| read_ace(acl, index))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(accept(&owner, control & SE_DACL_PROTECTED != 0, &aces))
}

/// Reads one ACE; only allow and deny ACEs carry a SID at the fixed offset, others get none.
fn read_ace(acl: *const ACL, index: u32) -> io::Result<AceView> {
    let mut raw = ptr::null_mut();
    // SAFETY: acl is live and index is within its AceCount.
    if unsafe { GetAce(acl, index, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let base = raw.cast::<u8>().cast_const();
    // SAFETY: GetAce returned a live ACE, and every ACE starts with a 4-byte `ACE_HEADER`.
    let header = unsafe { ptr::read_unaligned(base.cast::<ACE_HEADER>()) };
    let size = usize::from(header.AceSize);
    if size < offset_of!(ACCESS_ALLOWED_ACE, SidStart) {
        return Err(short_ace());
    }
    // SAFETY: `AceSize` covers the mask, which sits at the same offset in every ACE type.
    let mask = unsafe {
        ptr::read_unaligned(base.add(offset_of!(ACCESS_ALLOWED_ACE, Mask)).cast::<u32>())
    };
    let kind = header.AceType;
    let sid = if kind <= 1 {
        let start = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
        // A SID is 8 header bytes plus 4 per sub-authority; its count is the byte at offset 1.
        if size < start + 8 {
            return Err(short_ace());
        }
        // SAFETY: allow (0) and deny (1) ACEs store their SID at `SidStart`; 8 bytes are in bounds.
        let sid = unsafe { base.add(start) };
        // SAFETY: offset 1 is inside the 8 SID header bytes checked above.
        let count = usize::from(unsafe { sid.add(1).read() });
        if size < start + 8 + 4 * count {
            return Err(short_ace());
        }
        security::sid_text(sid.cast_mut().cast())?
    } else {
        String::new()
    };
    Ok(AceView {
        kind,
        flags: header.AceFlags,
        mask,
        sid,
    })
}

/// The error for an ACE whose `AceSize` does not cover its mask or SID; the check fails closed.
fn short_ace() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "ACE is shorter than its fields")
}

/// Lists the held root from its start; only `.` and `..` (or nothing) count as empty.
///
/// After an all-dots first batch it lists once more, since some file systems return the dots alone.
fn root_empty(handle: &Owned) -> Result<bool, PinError> {
    let first = list_batch(handle, FileIdBothDirectoryRestartInfo);
    match adopt_rule::root_emptiness(first, || list_batch(handle, FileIdBothDirectoryInfo)) {
        Emptiness::Empty => Ok(true),
        Emptiness::NotEmpty => Ok(false),
        Emptiness::Failed(error) => Err(error.into()),
        Emptiness::Malformed => Err(PinError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "folder listing is malformed",
        ))),
    }
}

/// Reads one `class` listing batch of the held folder into bytes; the end is `NoMoreFiles`.
fn list_batch(handle: &Owned, class: FILE_INFO_BY_HANDLE_CLASS) -> Batch<Vec<u8>, io::Error> {
    let mut words = vec![0_u64; LISTING_WORDS];
    let Ok(size) = u32::try_from(LISTING_WORDS * size_of::<u64>()) else {
        return Batch::Failed(io::Error::other("listing buffer too large"));
    };
    // SAFETY: the handle is live and `words` is a writable, 8-byte aligned buffer of `size` bytes.
    let ok =
        unsafe { GetFileInformationByHandleEx(handle.0, class, words.as_mut_ptr().cast(), size) };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_NO_MORE_FILES.cast_signed()) {
            Batch::NoMoreFiles
        } else {
            Batch::Failed(error)
        };
    }
    Batch::Entries(words.iter().flat_map(|word| word.to_ne_bytes()).collect())
}

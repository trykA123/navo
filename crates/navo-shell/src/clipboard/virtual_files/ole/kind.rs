use windows::Win32::System::Com::{CoTaskMemFree, IDataObject, TYMED_HGLOBAL};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{SHGetNameFromIDList, SIGDN_DESKTOPABSOLUTEPARSING};

use std::path::PathBuf;

use super::super::SourceKind;
use super::super::hints::{inside_zip, is_rdpclip, zip_file};
use super::medium::Medium;
use super::{Formats, Origin};

/// The cheap hints only: Outlook's private format, the Remote Desktop clipboard owner (the
/// system's `rdpclip.exe`, by full path), or a shell list whose folder lies inside a .zip file,
/// with that file's path when it is known.
pub(super) fn source_kind(
    object: &IDataObject,
    formats: &Formats,
    origin: &Origin,
) -> (SourceKind, Option<PathBuf>) {
    if formats.mail.is_some_and(|format| offers(object, format)) {
        return (SourceKind::Mail, None);
    }
    if matches!(origin, Origin::Clipboard) && from_rdpclip() {
        return (SourceKind::RemoteSession, None);
    }
    let folder = formats
        .id_list
        .and_then(|format| parent_name(object, format))
        .filter(|name| inside_zip(name));
    match folder {
        Some(name) => (SourceKind::Archive, zip_file(&name)),
        None => (SourceKind::Unknown, None),
    }
}

fn from_rdpclip() -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_default();
    crate::clipboard::watch::owner_exe_path().is_some_and(|exe| is_rdpclip(&exe, &root))
}

fn offers(object: &IDataObject, format: u16) -> bool {
    Medium::get(object, format, -1, TYMED_HGLOBAL).is_ok()
}

/// The parsing name of the folder in a CIDA ("Shell IDList Array").
fn parent_name(object: &IDataObject, format: u16) -> Option<String> {
    let medium = Medium::get(object, format, -1, TYMED_HGLOBAL).ok()?;
    let bytes = medium.global_bytes(usize::MAX).ok()?;
    let offset = usize::try_from(read_u32(&bytes, 4)?).ok()?;
    let pidl = bytes.get(offset..).filter(|pidl| terminated(pidl))?;
    // SAFETY: pidl is a complete, terminated item ID list inside bytes, alive for the call.
    let name = unsafe {
        SHGetNameFromIDList(
            pidl.as_ptr().cast::<ITEMIDLIST>(),
            SIGDN_DESKTOPABSOLUTEPARSING,
        )
    }
    .ok()?;
    // SAFETY: the shell returned a NUL-terminated string that this function now owns.
    let text = unsafe { name.to_string() }.ok();
    // SAFETY: the string came from the shell allocator and is freed once.
    unsafe { CoTaskMemFree(Some(name.0.cast())) };
    text
}

/// Whether the item ID list ends with its zero terminator inside `pidl`.
fn terminated(pidl: &[u8]) -> bool {
    let mut at = 0usize;
    while let Some(cb) = pidl.get(at..at + 2) {
        let cb = usize::from(u16::from_le_bytes([cb[0], cb[1]]));
        if cb == 0 {
            return true;
        }
        if cb < 2 {
            return false;
        }
        at = match at.checked_add(cb) {
            Some(next) => next,
            None => return false,
        };
    }
    false
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

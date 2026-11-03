use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::System::Com::StructuredStorage::{IStorage, StgCreateDocfile};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IDataObject, STGC_DEFAULT, STGM_FAILIFTHERE, STGM_READWRITE,
    STGM_SHARE_EXCLUSIVE, STGMEDIUM, TYMED, TYMED_HGLOBAL, TYMED_ISTORAGE, TYMED_ISTREAM,
};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::core::PCWSTR;
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

use super::super::descriptor::global_len;
use super::super::{ItemSource, Opened};

/// A medium from `GetData`, released exactly once.
pub(super) struct Medium(STGMEDIUM);

impl Drop for Medium {
    fn drop(&mut self) {
        // SAFETY: the medium came from a successful GetData and is released only here.
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

impl Medium {
    pub(super) fn get(
        object: &IDataObject,
        format: u16,
        index: i32,
        tymed: TYMED,
    ) -> windows::core::Result<Self> {
        let request = FORMATETC {
            cfFormat: format,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: index,
            tymed: u32::try_from(tymed.0).unwrap_or_default(),
        };
        // SAFETY: request is a valid FORMATETC that lives across the call.
        unsafe { object.GetData(&request) }.map(Self)
    }

    fn has(&self, tymed: TYMED) -> bool {
        u32::try_from(tymed.0).is_ok_and(|tymed| self.0.tymed == tymed)
    }

    /// The bytes of an HGLOBAL medium, at most `limit` of them.
    pub(super) fn global_bytes(&self, limit: usize) -> Result<Vec<u8>, String> {
        if !self.has(TYMED_HGLOBAL) {
            return Err("The source did not offer the data in memory".into());
        }
        let mut global = GlobalItem {
            medium: self,
            at: 0,
            limit,
        };
        let mut bytes = Vec::new();
        let mut buf = vec![0; 64 * 1024];
        loop {
            match global.read_into(&mut buf)? {
                0 => return Ok(bytes),
                read => bytes.extend_from_slice(&buf[..read]),
            }
        }
    }

    /// The allocated size of an HGLOBAL medium.
    fn global_size(&self) -> usize {
        // SAFETY: the caller checked that the medium holds an HGLOBAL, alive until release.
        let handle = unsafe { self.0.u.hGlobal }.0;
        // SAFETY: handle is a live HGLOBAL owned by the medium.
        unsafe { GlobalSize(handle) }
    }

    fn storage(&self) -> Option<&IStorage> {
        if !self.has(TYMED_ISTORAGE) {
            return None;
        }
        // SAFETY: tymed says the union holds an IStorage.
        unsafe { self.0.u.pstg.as_ref() }
    }
}

struct GlobalItem<'a> {
    medium: &'a Medium,
    at: usize,
    limit: usize,
}

impl GlobalItem<'_> {
    fn read_into(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        // SAFETY: the caller checked that the medium holds an HGLOBAL, alive until release.
        let handle = unsafe { self.medium.0.u.hGlobal }.0;
        // SAFETY: handle is a live HGLOBAL owned by the medium.
        let size = unsafe { GlobalSize(handle) }.min(self.limit);
        let count = size.saturating_sub(self.at).min(buf.len());
        if count == 0 {
            return Ok(0);
        }
        // SAFETY: handle is a live HGLOBAL; the lock is released below.
        let data = unsafe { GlobalLock(handle) }.cast::<u8>();
        if data.is_null() {
            return Err(format!(
                "Locking the file data: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: at + count <= GlobalSize, the block is locked and does not overlap buf.
        unsafe { std::ptr::copy_nonoverlapping(data.add(self.at), buf.as_mut_ptr(), count) };
        // SAFETY: this function locked the handle once above.
        unsafe { GlobalUnlock(handle) };
        self.at += count;
        Ok(count)
    }
}

struct OwnedGlobal {
    medium: Medium,
    at: usize,
    limit: usize,
}

impl ItemSource for OwnedGlobal {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        let mut item = GlobalItem {
            medium: &self.medium,
            at: self.at,
            limit: self.limit,
        };
        let read = item.read_into(buf)?;
        self.at = item.at;
        Ok(read)
    }
}

struct StreamItem(Medium);

impl ItemSource for StreamItem {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        // SAFETY: StreamItem is built only for an ISTREAM medium.
        let stream = unsafe { self.0.0.u.pstm.as_ref() }.ok_or("The file data has no stream")?;
        let wanted = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let mut read = 0u32;
        // SAFETY: buf holds at least `wanted` bytes and `read` outlives the call.
        let result = unsafe { stream.Read(buf.as_mut_ptr().cast(), wanted, Some(&mut read)) };
        result.ok().map_err(|error| error.message())?;
        Ok(usize::try_from(read).unwrap_or(0).min(buf.len()))
    }
}

/// FileContents for one item, through whichever medium the source picks.
pub(super) fn open(
    object: &IDataObject,
    (format, index): (u16, i32),
    size: Option<u64>,
) -> Result<Opened, String> {
    let any = TYMED(TYMED_ISTREAM.0 | TYMED_HGLOBAL.0 | TYMED_ISTORAGE.0);
    let medium = Medium::get(object, format, index, any).map_err(|error| error.message())?;
    if medium.has(TYMED_ISTREAM) {
        return Ok(Opened::Stream(Box::new(StreamItem(medium))));
    }
    if medium.has(TYMED_HGLOBAL) {
        let limit = global_len(size, medium.global_size())?;
        let item = OwnedGlobal {
            medium,
            at: 0,
            limit,
        };
        return Ok(Opened::Stream(Box::new(item)));
    }
    if medium.has(TYMED_ISTORAGE) {
        return Ok(Opened::Storage);
    }
    Err("The source offered the file in a form Navo cannot read".into())
}

/// Saves a storage item as a new compound file; a partial file is removed on failure.
pub(super) fn write_storage(
    object: &IDataObject,
    (format, index): (u16, i32),
    path: &Path,
    created: &mut dyn FnMut(),
) -> Result<(), String> {
    let medium =
        Medium::get(object, format, index, TYMED_ISTORAGE).map_err(|error| error.message())?;
    let source = medium
        .storage()
        .ok_or("The source did not offer a storage")?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    if wide[..wide.len() - 1].contains(&0) {
        return Err("The target path is not valid".into());
    }
    let mode = STGM_READWRITE | STGM_SHARE_EXCLUSIVE | STGM_FAILIFTHERE;
    // SAFETY: wide is NUL-terminated and outlives the call; FAILIFTHERE never overwrites.
    let target = unsafe { StgCreateDocfile(PCWSTR(wide.as_ptr()), mode, None) }
        .map_err(|error| error.message())?;
    created();
    let copied = commit_copy(source, &target);
    drop(target);
    let flushed = copied
        .map_err(|error| error.message())
        .and_then(|()| flush(path));
    if flushed.is_err() {
        let _ = std::fs::remove_file(path);
    }
    flushed
}

fn commit_copy(source: &IStorage, target: &IStorage) -> windows::core::Result<()> {
    // SAFETY: both storages are live and CopyTo takes no other pointers.
    unsafe { source.CopyTo(None, None, target) }?;
    let flags = u32::try_from(STGC_DEFAULT.0).unwrap_or(0);
    // SAFETY: target is a live storage this thread created.
    unsafe { target.Commit(flags) }
}

fn flush(path: &Path) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

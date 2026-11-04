//! Whether a user command's program has a certain identity.

#![allow(unsafe_code, reason = "the reparse point probe needs Win32")]

use std::path::Path;

use crate::editor::{Search, clean, resolve};

/// The message a placeholder gets when the program is a link or alias.
pub(super) const UNKNOWN_PROGRAM: &str =
    "the program name cannot be verified; placeholders are not allowed";

/// How the program file itself appears on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileKind {
    Real,
    Uncertain,
}

#[cfg(test)]
thread_local! {
    static PROBE: std::cell::Cell<Option<FileKind>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
struct ProbeRestore(Option<FileKind>);

#[cfg(test)]
impl Drop for ProbeRestore {
    fn drop(&mut self) {
        PROBE.with(|slot| slot.set(self.0));
    }
}

#[cfg(test)]
pub(super) fn with_probe<T>(kind: FileKind, run: impl FnOnce() -> T) -> T {
    let previous = PROBE.with(|slot| slot.replace(Some(kind)));
    let _restore = ProbeRestore(previous);
    run()
}

/// Whether the program is a link, alias or shortcut.
pub(super) fn uncertain(path: &Path, probe: impl FnOnce(&Path) -> FileKind) -> bool {
    is_shortcut(path) || probe(path) == FileKind::Uncertain
}

/// The load-time check, using the same resolver and probe as runtime.
///
/// A program that is not found loads; running it is refused with `ProgramNotFound`.
pub(super) fn load_uncertain(program: &str, search: &Search) -> bool {
    resolve(&clean(program), search).is_some_and(|path| resolved_uncertain(&path))
}

/// Whether the program resolves to a shell, such as `code.cmd`.
pub(super) fn load_shell(program: &str, search: &Search) -> bool {
    resolve(&clean(program), search).is_some_and(|path| super::expand::resolved_is_shell(&path))
}

/// Whether the resolved program file is a link, alias or shortcut.
pub(super) fn resolved_uncertain(path: &Path) -> bool {
    uncertain(path, probe)
}

fn probe(path: &Path) -> FileKind {
    #[cfg(test)]
    if let Some(kind) = PROBE.with(std::cell::Cell::get) {
        return kind;
    }
    file_kind(path)
}

fn is_shortcut(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
}

#[cfg(windows)]
fn file_kind(path: &Path) -> FileKind {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: wide is NUL-terminated; the call opens the reparse point itself.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return FileKind::Uncertain;
    }
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: handle is live and info is writable output storage.
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    // SAFETY: handle was opened here and is closed once.
    unsafe { CloseHandle(handle) };
    if ok == 0 || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        FileKind::Uncertain
    } else {
        FileKind::Real
    }
}

#[cfg(unix)]
fn file_kind(path: &Path) -> FileKind {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => FileKind::Uncertain,
        Ok(_) => FileKind::Real,
        Err(_) => FileKind::Uncertain,
    }
}

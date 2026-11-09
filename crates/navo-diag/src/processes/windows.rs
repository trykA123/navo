// Windows process scan: a ToolHelp snapshot names each image, and open processes give their path.
#![allow(unsafe_code, reason = "process enumeration requires Win32 FFI")]

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

const PATH_UNITS: usize = 32_768;

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn any_under(root: &Path) -> io::Result<bool> {
    let mut roots = vec![normalized(root)];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        roots.push(normalized(&canonical));
    }
    let own_name = std::env::current_exe()?
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .ok_or_else(|| io::Error::other("own image has no name"))?;
    let own = std::process::id();
    let own_image = Own {
        name: own_name,
        session: session(own),
    };
    let size = u32::try_from(size_of::<PROCESSENTRY32W>()).map_err(io::Error::other)?;
    // SAFETY: no pointers are passed; the result is checked before use.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Owned(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size,
        ..PROCESSENTRY32W::default()
    };
    let mut path = vec![0u16; PATH_UNITS];
    // SAFETY: the snapshot handle is live and `entry` is writable with dwSize set.
    if unsafe { Process32FirstW(snapshot.0, &raw mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }
    loop {
        if super::is_other(entry.th32ProcessID, entry.th32ParentProcessID, own)
            && belongs(&entry, &mut path, &roots, &own_image)
        {
            return Ok(true);
        }
        entry.dwSize = size;
        // SAFETY: as above; the snapshot stays open until `snapshot` drops.
        if unsafe { Process32NextW(snapshot.0, &raw mut entry) } == 0 {
            return end_of_list(io::Error::last_os_error());
        }
    }
}

/// The end of the snapshot is "nobody found"; any other error is a failure.
fn end_of_list(error: io::Error) -> io::Result<bool> {
    if error.raw_os_error() == i32::try_from(ERROR_NO_MORE_FILES).ok() {
        Ok(false)
    } else {
        Err(error)
    }
}

/// This process's image name and Windows session.
struct Own {
    name: String,
    session: Option<u32>,
}

/// A process whose image cannot be read counts when its name looks like a Navo image.
fn belongs(entry: &PROCESSENTRY32W, path: &mut [u16], roots: &[PathBuf], own: &Own) -> bool {
    match image(entry.th32ProcessID, path) {
        Some(image) => {
            let image = normalized(&image);
            roots.iter().any(|root| image.starts_with(root))
        }
        None => {
            let name = entry
                .szExeFile
                .split(|unit| *unit == 0)
                .next()
                .unwrap_or_default();
            let same_session = own.session.is_some() && session(entry.th32ProcessID) == own.session;
            super::unreadable_counts(&String::from_utf16_lossy(name), &own.name, same_session)
        }
    }
}

fn session(pid: u32) -> Option<u32> {
    let mut session = 0u32;
    // SAFETY: `session` is a writable u32 for the duration of the call.
    let success = unsafe { ProcessIdToSessionId(pid, &raw mut session) };
    (success != 0).then_some(session)
}

fn image(pid: u32, path: &mut [u16]) -> Option<PathBuf> {
    // SAFETY: requests only limited query access for a process ID, with no inherited handle.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let process = Owned(handle);
    let mut length = u32::try_from(path.len()).ok()?;
    // SAFETY: the process handle is live and `path` has `length` writable UTF-16 units.
    let success = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &raw mut length,
        )
    };
    if success == 0 {
        return None;
    }
    let length = usize::try_from(length).ok()?;
    Some(PathBuf::from(OsString::from_wide(path.get(..length)?)))
}

/// Lowercase, without the `\\?\` prefix, so the two path forms compare by component.
fn normalized(path: &Path) -> PathBuf {
    let text = path.to_string_lossy().to_lowercase();
    let text = match text.strip_prefix(r"\\?\unc\") {
        Some(rest) => format!(r"\\{rest}"),
        None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned(),
    };
    PathBuf::from(text)
}

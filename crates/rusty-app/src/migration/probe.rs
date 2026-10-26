use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

use rusty_core::brand;
use rusty_store::StoreDirs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Legacy {
    Running,
    Closed,
}

pub(crate) fn legacy(old: &StoreDirs) -> Legacy {
    probe(old, instance_running)
}

pub(super) fn probe(old: &StoreDirs, instance: impl FnOnce(&str) -> bool) -> Legacy {
    let hash = crate::instance::profile_hash(&old.data);
    let running = match &hash {
        Ok(hash) => instance(hash),
        Err(error) => {
            tracing::warn!(%error, "cannot name the old instance lock");
            false
        }
    };
    if running || is_locked(&old.legacy_lock_file()) {
        Legacy::Running
    } else {
        Legacy::Closed
    }
}

pub(super) fn is_locked(path: &Path) -> bool {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "cannot open the old lock");
            return false;
        }
    };
    match file.try_lock() {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(error)) => {
            tracing::warn!(%error, path = %path.display(), "cannot test the old lock");
            false
        }
    }
}

#[cfg(unix)]
fn instance_running(hash: &str) -> bool {
    let runtime = crate::instance::runtime_folder(brand::LEGACY_SLUG);
    is_locked(&lock_in(&runtime, hash))
}

#[cfg(unix)]
pub(super) fn lock_in(runtime: &Path, hash: &str) -> std::path::PathBuf {
    runtime.join(format!("{}-{hash}.lock", brand::LEGACY_SLUG))
}

#[cfg(windows)]
pub(super) fn mutex_name(hash: &str) -> Vec<u16> {
    format!("Local\\{}-{hash}", brand::LEGACY_NAME)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn instance_running(hash: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    let name = mutex_name(hash);
    // SAFETY: the name is NUL-terminated and outlives the call; no handle is inherited.
    let handle = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) };
    if handle.is_null() {
        // SAFETY: GetLastError reads this thread's error immediately after OpenMutexW.
        return unsafe { GetLastError() } != ERROR_FILE_NOT_FOUND;
    }
    // SAFETY: OpenMutexW returned a valid handle that this function owns.
    unsafe { CloseHandle(handle) };
    true
}

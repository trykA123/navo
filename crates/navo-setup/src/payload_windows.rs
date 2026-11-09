//! Private Windows work folders, pinned extraction and identity-checked cleanup.

use super::{LogEntry, Payload, logs_to_remove, verify_hash};
use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED, FILETIME, GENERIC_READ, SYSTEMTIME,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{
        CreateDirectoryW, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO,
        FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FileAttributeTagInfo, FileDispositionInfo, FileDispositionInfoEx, FileIdInfo,
        GetFileInformationByHandleEx, SetFileInformationByHandle,
    },
    System::{SystemInformation::GetSystemTimeAsFileTime, Time::FileTimeToSystemTime},
};

#[path = "payload_security.rs"]
mod security;

/// A volume plus 128-bit file identity, never a pathname comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    volume: u64,
    id: [u8; 16],
}

/// An owned private folder with explicit cleanup and a best-effort drop fallback.
#[derive(Debug)]
pub struct Work {
    pub path: PathBuf,
    directory: Option<File>,
    identity: Identity,
    setup: Cell<Option<Identity>>,
    cleanup_attempted: bool,
}
impl Work {
    /// Creates a private random folder under TEMP; collisions retry without touching existing data.
    pub fn create() -> io::Result<Self> {
        Self::create_in(&std::env::temp_dir(), &mut security::random_name)
    }

    /// Creates a work folder under a supplied test root, trying at most sixteen names.
    fn create_in(root: &Path, next: &mut impl FnMut() -> io::Result<String>) -> io::Result<Self> {
        let descriptor = security::private_descriptor()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(io::Error::other)?,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        for _ in 0..16 {
            let path = root.join(format!("NavoSetup-{}", next()?));
            let wide = wide(&path);
            // SAFETY: path and the descriptor/attributes remain live throughout the call.
            let created = unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } != 0;
            match (!created).then(io::Error::last_os_error) {
                Some(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Some(error) => return Err(error),
                None => {}
            }
            let directory = open(
                &path,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            )?;
            let identity = identity(&directory)?;
            return Ok(Self {
                path,
                directory: Some(directory),
                identity,
                setup: Cell::new(None),
                cleanup_attempted: false,
            });
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "work-folder name collisions",
        ))
    }

    /// Closes the extracted handle before deleting recorded files; changed identities are retained.
    pub fn cleanup(mut self, extracted: Option<Extracted>) -> io::Result<bool> {
        drop(extracted);
        self.cleanup_recorded()
    }

    /// Deletes owned paths once, keeping the directory pinned until its files are removed.
    fn cleanup_recorded(&mut self) -> io::Result<bool> {
        self.cleanup_attempted = true;
        if let Some(expected) = self.setup.get()
            && !delete_recorded(&self.path.join("Setup.exe"), Some(expected))?
        {
            return Ok(false);
        }
        drop(self.directory.take());
        delete_recorded(&self.path, Some(self.identity))
    }
}
impl Drop for Work {
    /// Attempts the same checked cleanup when explicit cleanup was never called; ignores errors.
    fn drop(&mut self) {
        if !self.cleanup_attempted {
            let _ = self.cleanup_recorded();
        }
    }
}

/// Setup.exe with a read-only, read-share-only handle kept until the child exits.
#[derive(Debug)]
pub struct Extracted {
    pub path: PathBuf,
    _held: File,
}
impl Payload {
    /// Writes CREATE_NEW bytes, reopens read-only, and hashes that held handle before use.
    pub fn extract(&self, work: &Work) -> io::Result<Extracted> {
        if work.setup.get().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "payload already extracted",
            ));
        }
        let path = work.path.join("Setup.exe");
        let mut writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)?;
        work.setup.set(Some(identity(&writer)?));
        writer.write_all(&self.setup)?;
        writer.sync_all()?;
        drop(writer);
        let mut held = open(&path, GENERIC_READ, FILE_SHARE_READ)?;
        if Some(identity(&held)?) != work.setup.get() {
            return Err(io::Error::other("extracted payload identity changed"));
        }
        verify_hash(&mut held, &self.setup_sha256, self.setup_size)?;
        Ok(Extracted { path, _held: held })
    }
}

/// Opens a file or directory without following its final reparse point.
fn open(path: &Path, access: u32, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(share)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

/// Reads a handle's file identity and refuses every reparse tag.
fn identity(file: &File) -> io::Result<Identity> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    read_info(file, FileAttributeTagInfo, &mut tag)?;
    if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0 {
        return Err(io::Error::other("payload path is a reparse point"));
    }
    let mut info = FILE_ID_INFO::default();
    read_info(file, FileIdInfo, &mut info)?;
    Ok(Identity {
        volume: info.VolumeSerialNumber,
        id: info.FileId.Identifier,
    })
}

/// Fills one correctly sized Win32 information structure from a live file handle.
fn read_info<T>(file: &File, class: i32, output: &mut T) -> io::Result<()> {
    let size = u32::try_from(size_of::<T>()).map_err(io::Error::other)?;
    // SAFETY: callers pair the information class with its matching writable structure.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            class,
            ptr::from_mut(output).cast(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Reopens without following reparses, checks any recorded identity and deletes on that handle.
fn delete_recorded(path: &Path, expected: Option<Identity>) -> io::Result<bool> {
    let file = match open(
        path,
        DELETE | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error),
    };
    let actual = identity(&file)?;
    if expected.is_some_and(|expected| actual != expected) {
        return Ok(false);
    }
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    let size = u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>()).map_err(io::Error::other)?;
    // SAFETY: file has DELETE access; disposition matches FileDispositionInfoEx and stays live.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            ptr::from_ref(&disposition).cast(),
            size,
        )
    } != 0
    {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if !matches!(error.raw_os_error(), Some(code) if code == ERROR_INVALID_PARAMETER as i32 || code == ERROR_NOT_SUPPORTED as i32)
    {
        return Err(error);
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let size = u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).map_err(io::Error::other)?;
    // SAFETY: file has DELETE access; disposition is initialized and matches its information class.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            ptr::from_ref(&disposition).cast(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(true)
}

/// Separate wizard and Velopack paths belonging to the same UTC run.
#[derive(Debug)]
pub struct Logs {
    pub wizard: PathBuf,
    pub velopack: PathBuf,
}
impl Logs {
    /// Creates the TEMP log directory, rotates old runs and chooses UTC names with milliseconds.
    pub fn create() -> io::Result<Self> {
        let directory = std::env::temp_dir().join("NavoSetup-logs");
        fs::create_dir_all(&directory)?;
        rotate_logs(&directory)?;
        let mut time = FILETIME::default();
        // SAFETY: time is writable; GetSystemTimeAsFileTime returns UTC and cannot fail.
        unsafe { GetSystemTimeAsFileTime(&mut time) };
        let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        for attempt in 0..5 {
            let ticks = ticks + attempt * 10_000;
            let time = FILETIME {
                dwLowDateTime: u32::try_from(ticks & 0xffff_ffff).map_err(io::Error::other)?,
                dwHighDateTime: u32::try_from(ticks >> 32).map_err(io::Error::other)?,
            };
            let mut utc = SYSTEMTIME::default();
            // SAFETY: time is initialized and utc is a writable SYSTEMTIME.
            if unsafe { FileTimeToSystemTime(&time, &mut utc) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let stem = format!(
                "{:04}{:02}{:02}T{:02}{:02}{:02}{:03}Z",
                utc.wYear,
                utc.wMonth,
                utc.wDay,
                utc.wHour,
                utc.wMinute,
                utc.wSecond,
                utc.wMilliseconds
            );
            let wizard = directory.join(format!("{stem}.log"));
            match File::create_new(&wizard) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            let logs = Self {
                wizard,
                velopack: directory.join(format!("{stem}-velopack.log")),
            };
            rotate_logs(&directory)?;
            return Ok(logs);
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "log name collisions",
        ))
    }
}

/// Keeps five runs, skipping reparse points and deleting older files only by checked handles.
pub fn rotate_logs(directory: &Path) -> io::Result<()> {
    let held = open(
        directory,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    read_info(&held, FileAttributeTagInfo, &mut tag)?;
    if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0 {
        return Ok(());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let Ok(entry) = entry else { continue };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        entries.push(LogEntry {
            path: entry.path(),
            reparse: metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || !metadata.is_file(),
        });
    }
    for path in logs_to_remove(&entries) {
        let _ = delete_recorded(&path, None);
    }
    Ok(())
}

/// Encodes a path as a NUL-terminated UTF-16 string for directory creation.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
#[path = "payload_windows_tests.rs"]
mod tests;

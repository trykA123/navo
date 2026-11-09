//! Win32 implementation of upgrade staging, Update.exe replacement and cleanup.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "package staging, file identity and Update.exe replacement require Win32 FFI"
)]

use std::fs::{File, OpenOptions};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FileAttributeTagInfo, FileDispositionInfo, FileIdInfo,
    GetFileInformationByHandleEx, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    SetFileInformationByHandle,
};

use crate::payload::{Payload, verify_hash};
use crate::placement::FileId;

const OPEN_REPARSE: u32 = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT;
const PACKAGE_SUFFIX: &str = ".navo-partial";
const UPDATE_SUFFIX: &str = ".navo-new";

/// Writes the embedded nupkg through a hashed partial file and renames it into place.
pub(super) fn stage_package(root: &Path, payload: &Payload) -> io::Result<PathBuf> {
    let packages = ensure_packages(root)?;
    let final_path = packages.join(&payload.package_name);
    ensure_replaceable(&final_path)?;
    let partial = packages.join(format!("{}{PACKAGE_SUFFIX}", payload.package_name));
    delete_leftover(&partial)?;
    let mut file = create_new(&partial)?;
    let id = identity(&file)?.0;
    let result = write_package(&mut file, payload);
    drop(file);
    if let Err(error) = result {
        let _ = delete_recorded(&partial, id);
        return Err(error);
    }
    if let Err(error) = move_replace(&partial, &final_path) {
        let _ = delete_recorded(&partial, id);
        return Err(error);
    }
    delete_other_packages(&packages, &payload.package_name)?;
    Ok(final_path)
}

/// Extracts the single Squirrel.exe entry over Update.exe and returns its SHA-256.
pub(super) fn replace_update_exe(root: &Path, payload: &Payload) -> io::Result<[u8; 32]> {
    let update = root.join("Update.exe");
    ensure_update(&update)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(payload.package()))
        .map_err(|_| io::Error::other("damaged package"))?;
    let (index, size) = find_squirrel(&mut archive)?;
    let new = root.join(format!("Update.exe{UPDATE_SUFFIX}"));
    delete_leftover(&new)?;
    let mut file = create_new(&new)?;
    let id = identity(&file)?.0;
    let hash = extract_squirrel(&mut archive, index, size, &mut file);
    drop(file);
    let hash = match hash {
        Ok(hash) => hash,
        Err(error) => {
            let _ = delete_recorded(&new, id);
            return Err(error);
        }
    };
    if let Err(error) = move_replace(&new, &update) {
        let _ = delete_recorded(&new, id);
        return Err(error);
    }
    Ok(hash)
}

/// Writes all package bytes, flushes, then verifies the recorded hash from the start.
fn write_package(file: &mut File, payload: &Payload) -> io::Result<()> {
    file.write_all(payload.package())?;
    file.sync_all()?;
    file.seek(SeekFrom::Start(0))?;
    verify_hash(file, &payload.package_sha256, payload.package_size)
}

/// Creates `packages\` if absent and refuses a file or reparse point there.
fn ensure_packages(root: &Path) -> io::Result<PathBuf> {
    let path = root.join("packages");
    let wide = wide(&path);
    // SAFETY: the path is NUL-terminated and no security attributes are used.
    if unsafe { CreateDirectoryW(wide.as_ptr(), std::ptr::null()) } == 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    let file = open_file(
        &path,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let (_, reparse, directory) = identity(&file)?;
    if reparse || !directory {
        return Err(io::Error::other("packages path is not a folder"));
    }
    Ok(path)
}

/// Accepts an absent or regular destination; a directory or reparse point refuses.
fn ensure_replaceable(path: &Path) -> io::Result<()> {
    match open_file(
        path,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) {
        Ok(file) => {
            let (_, reparse, directory) = identity(&file)?;
            if reparse || directory {
                return Err(io::Error::other(
                    "package destination is not a regular file",
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Refuses a missing, directory or reparse Update.exe.
fn ensure_update(path: &Path) -> io::Result<()> {
    let file = open_file(
        path,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?;
    let (_, reparse, directory) = identity(&file)?;
    if reparse || directory {
        return Err(io::Error::other("Update.exe is not a regular file"));
    }
    Ok(())
}

/// Finds exactly one Squirrel.exe entry and returns its index and size.
fn find_squirrel(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> io::Result<(usize, u64)> {
    let mut found = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| io::Error::other("damaged package"))?;
        if entry.is_dir() {
            continue;
        }
        if entry
            .name()
            .map_err(|_| io::Error::other("damaged package"))?
            .ends_with("Squirrel.exe")
        {
            if found.is_some() {
                return Err(io::Error::other(
                    "package has multiple Squirrel.exe entries",
                ));
            }
            found = Some((index, entry.size()));
        }
    }
    found.ok_or_else(|| io::Error::other("package has no Squirrel.exe entry"))
}

/// Copies one archive entry into the new Update.exe while hashing and checking its size.
fn extract_squirrel(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    index: usize,
    size: u64,
    file: &mut File,
) -> io::Result<[u8; 32]> {
    let mut entry = archive
        .by_index(index)
        .map_err(|_| io::Error::other("damaged package"))?;
    if entry.size() != size {
        return Err(io::Error::other("package entry changed size"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = entry.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        total += u64::try_from(count).map_err(io::Error::other)?;
    }
    if total != size {
        return Err(io::Error::other("package entry is truncated"));
    }
    file.sync_all()?;
    Ok(hash.finalize().into())
}

/// Deletes every other nupkg or partial file in packages, never following a reparse.
fn delete_other_packages(packages: &Path, keep: &str) -> io::Result<()> {
    for entry in std::fs::read_dir(packages)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.eq_ignore_ascii_case(keep) {
            continue;
        }
        if name.ends_with(".navo-partial") {
            delete_leftover(&entry.path())?;
        } else if name.ends_with(".nupkg") || name.ends_with(".partial") {
            delete_regular(&entry.path())?;
        }
    }
    Ok(())
}

/// Removes an exact leftover file, refusing a directory or reparse point.
fn delete_leftover(path: &Path) -> io::Result<()> {
    if delete_regular(path)? {
        Ok(())
    } else {
        Err(io::Error::other("leftover path is not a regular file"))
    }
}

/// Opens a path without following its final reparse point.
fn open_file(path: &Path, access: u32, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(share)
        .custom_flags(OPEN_REPARSE)
        .open(path)
}

/// Creates a new file with no sharing, so a failed staging can be identity-checked.
fn create_new(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(path)
}

/// Reads a handle's identity, reparse flag and directory flag.
fn identity(file: &File) -> io::Result<(FileId, bool, bool)> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    read_info(file, FileAttributeTagInfo, &mut tag)?;
    let mut info = FILE_ID_INFO::default();
    read_info(file, FileIdInfo, &mut info)?;
    let reparse = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0;
    let directory = tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let id = FileId {
        volume: info.VolumeSerialNumber,
        id: u128::from_le_bytes(info.FileId.Identifier),
    };
    Ok((id, reparse, directory))
}

/// Fills one correctly sized Win32 information structure from a live file handle.
fn read_info<T>(file: &File, class: i32, output: &mut T) -> io::Result<()> {
    let size = u32::try_from(size_of::<T>()).map_err(io::Error::other)?;
    // SAFETY: callers pair the information class with its matching writable structure.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            class,
            std::ptr::from_mut(output).cast(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Deletes a regular file with no identity check; a directory or reparse is kept.
fn delete_regular(path: &Path) -> io::Result<bool> {
    let file = match open_file(
        path,
        DELETE | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error),
    };
    let (_, reparse, directory) = identity(&file)?;
    if reparse || directory {
        return Ok(false);
    }
    set_disposition(&file)?;
    drop(file);
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::other("package file still exists after deletion"));
    }
    Ok(true)
}

/// Deletes only the file with the recorded identity.
fn delete_recorded(path: &Path, expected: FileId) -> io::Result<bool> {
    let file = match open_file(
        path,
        DELETE | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error),
    };
    let (id, reparse, directory) = identity(&file)?;
    if reparse || directory || id != expected {
        return Ok(false);
    }
    set_disposition(&file)?;
    drop(file);
    Ok(std::fs::symlink_metadata(path).is_err())
}

/// Marks a handle's file for deletion when it closes.
fn set_disposition(file: &File) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let size = u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).map_err(io::Error::other)?;
    // SAFETY: the handle is live and `info` is a readable struct of the size passed.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            std::ptr::from_ref(&info).cast(),
            size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Moves a file into place with replace and write-through, keeping the old file on failure.
fn move_replace(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both paths are NUL-terminated and live for the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A NUL-terminated UTF-16 path for the Win32 calls.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

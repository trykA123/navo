//! Windows discovery: the Velopack registry root and the legacy fallback folder.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the uninstall registry value and Update.exe attributes require Win32 FFI"
)]

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use navo_core::{FileIdentity, FileIdentityKind};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW,
    INVALID_FILE_ATTRIBUTES,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData;

use crate::marker::Marker;
use crate::mode::{Discovery, Install, ModeError, SqVersion, Why, parse_sq_version};
use crate::placement::{Chain, DriveKind, Facts, FileId, Refused, inside};
use crate::probe::chain::{OrdinalNames, known_folder, refused, resolve_path};
use crate::probe::facts::facts as read_facts;
use crate::syntax::parse_candidate;

/// The per-user Velopack uninstall key.
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\NavoApp";

/// The largest `InstallLocation` value accepted, in bytes.
const MAX_LOCATION_BYTES: usize = 4 * 1024;

/// `current\sq.version` is read with IW8's 64 KiB cap.
const SQ_LIMIT: u64 = 64 * 1024;

/// `navo-install.json` is read with IW7's 4 KiB cap.
const MARKER_LIMIT: u64 = 4 * 1024;

/// Discovers the registry install and the legacy fallback folder.
pub fn discover() -> io::Result<Discovery> {
    let refused = refused()?;
    Ok(Discovery {
        registry: registry_install(&refused),
        legacy: legacy_install(&refused)?,
    })
}

/// Reads the registry key and turns its location into a discovery result.
fn registry_install(refused: &Refused) -> Option<Result<Install, Why>> {
    let path = match registry_location() {
        RegistryLocation::Absent => return None,
        RegistryLocation::Invalid => return Some(Err(Why::InvalidFolder)),
        RegistryLocation::Path(path) => path,
    };
    if let Some(character) = path.to_string_lossy().chars().find(|c| "%^&!".contains(*c)) {
        return Some(Err(Why::UnsafeName {
            root: path,
            character,
        }));
    }
    match install_at(&path, refused) {
        Ok(Some(install)) => Some(Ok(install)),
        Ok(None) | Err(_) => Some(Err(Why::InvalidFolder)),
    }
}

/// Reads `%LocalAppData%\NavoApp`; absent or incomplete leftovers stay `None`.
fn legacy_install(refused: &Refused) -> io::Result<Option<Install>> {
    let Some(base) = known_folder(&FOLDERID_LocalAppData)? else {
        return Ok(None);
    };
    match install_at(&base.join("NavoApp"), refused) {
        Ok(install) => Ok(install),
        Err(_) => Ok(None),
    }
}

/// Fills one `Install` from the chain, facts, files and marker; `None` when the root is absent.
fn install_at(root: &Path, refused: &Refused) -> io::Result<Option<Install>> {
    let chain = resolve_path(root)?;
    if !chain.missing.is_empty() {
        return Ok(None);
    }
    let Some(root_link) = chain.existing.last() else {
        return Ok(None);
    };
    let facts = read_facts(&chain)?;
    Ok(Some(Install {
        root: root.to_path_buf(),
        identity: identity(root_link.id),
        safe_chain: chain_is_safe(root, &chain, refused, &facts),
        update_exe: regular_file(&root.join("Update.exe")),
        sq_version: read_sq_version(&root.join("current").join("sq.version")),
        marker: read_marker_file(&root.join("navo-install.json")),
    }))
}

/// Checks IW6, IW7 and IW8a without requiring the folder to be writable or empty.
fn chain_is_safe(root: &Path, chain: &Chain, refused: &Refused, facts: &Facts) -> bool {
    if parse_candidate(root.as_os_str()).is_err() {
        return false;
    }
    if chain.existing.iter().any(|link| link.reparse)
        || facts.subst
        || !matches!(facts.drive, DriveKind::Fixed | DriveKind::Removable)
    {
        return false;
    }
    let names = OrdinalNames;
    if refused
        .never_inside
        .iter()
        .any(|folder| inside(chain, folder, &names))
    {
        return false;
    }
    let Some(root_link) = chain.existing.last() else {
        return false;
    };
    if refused
        .never_equal
        .iter()
        .any(|folder| folder.anchor == root_link.id && folder.tail.is_empty())
    {
        return false;
    }
    // A case-sensitive or unknown anchor refuses every candidate whose chain holds it.
    refused
        .never_inside
        .iter()
        .chain(&refused.never_equal)
        .all(|folder| {
            folder.anchor_case_sensitive == Some(false)
                || !chain.existing.iter().any(|link| link.id == folder.anchor)
        })
}

/// Maps a chain identity to the discovery identity.
fn identity(id: FileId) -> FileIdentity {
    FileIdentity {
        volume: id.volume,
        file: id.id.to_le_bytes(),
        kind: FileIdentityKind::Native,
    }
}

/// Whether a path is a regular file, not a directory or reparse point.
pub(crate) fn regular_file(path: &Path) -> bool {
    let path = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and live for the call.
    let attributes = unsafe { GetFileAttributesW(path.as_ptr()) };
    attributes != INVALID_FILE_ATTRIBUTES
        && attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0
}

/// Reads `current\sq.version` through the bounded parser.
pub(crate) fn read_sq_version(path: &Path) -> Result<SqVersion, ModeError> {
    match read_limited(path, SQ_LIMIT) {
        Ok(Limited::Bytes(bytes)) => {
            let text = String::from_utf8(bytes).map_err(|_| ModeError::Xml)?;
            parse_sq_version(&text)
        }
        Ok(Limited::TooLarge) => Err(ModeError::TooLarge),
        Ok(Limited::Missing) | Err(_) => Err(ModeError::Metadata),
    }
}

/// Reads `navo-install.json`; absent is `None`, every other failure is an error.
fn read_marker_file(path: &Path) -> Option<Result<Marker, ModeError>> {
    match read_limited(path, MARKER_LIMIT) {
        Ok(Limited::Missing) => None,
        Ok(Limited::TooLarge) => Some(Err(ModeError::TooLarge)),
        Ok(Limited::Bytes(bytes)) => Some(
            String::from_utf8(bytes)
                .map_err(|_| ModeError::Marker)
                .and_then(|text| Marker::parse(&text)),
        ),
        Err(_) => Some(Err(ModeError::Marker)),
    }
}

/// Reads a marker file, mapping a damaged document to an I/O error.
pub(crate) fn read_marker(path: &Path) -> io::Result<Option<Marker>> {
    match read_marker_file(path) {
        None => Ok(None),
        Some(Ok(marker)) => Ok(Some(marker)),
        Some(Err(error)) => Err(io::Error::other(error)),
    }
}

/// Reads the registry install location; absent is `None`, any other state is an error.
pub(crate) fn registry_root() -> io::Result<Option<PathBuf>> {
    match registry_location() {
        RegistryLocation::Absent => Ok(None),
        RegistryLocation::Invalid => Err(io::Error::other("uninstall registry key is invalid")),
        RegistryLocation::Path(path) => Ok(Some(path)),
    }
}

/// The result of a bounded file read.
enum Limited {
    Missing,
    TooLarge,
    Bytes(Vec<u8>),
}

/// Reads at most `cap` bytes, refusing anything larger.
fn read_limited(path: &Path, cap: u64) -> io::Result<Limited> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Limited::Missing),
        Err(error) => return Err(error),
    };
    if file.metadata()?.len() > cap {
        return Ok(Limited::TooLarge);
    }
    let mut bytes = Vec::new();
    let mut limited = file.take(cap.saturating_add(1));
    limited.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Ok(Limited::TooLarge);
    }
    Ok(Limited::Bytes(bytes))
}

/// The registry key state: absent, present but unusable, or a location.
enum RegistryLocation {
    Absent,
    Invalid,
    Path(PathBuf),
}

/// Reads `InstallLocation` from the per-user uninstall key.
fn registry_location() -> RegistryLocation {
    let key = match open_uninstall_key() {
        KeyOpen::Absent => return RegistryLocation::Absent,
        KeyOpen::Invalid => return RegistryLocation::Invalid,
        KeyOpen::Open(key) => key,
    };
    match query_location(&key) {
        Some(path) => RegistryLocation::Path(path),
        None => RegistryLocation::Invalid,
    }
}

/// Opens the per-user uninstall key, or reports that it is absent or unreadable.
fn open_uninstall_key() -> KeyOpen {
    let key_name = wide(UNINSTALL_KEY);
    let mut raw: HKEY = std::ptr::null_mut();
    // SAFETY: the key name is NUL-terminated and `raw` is a writable key pointer.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            KEY_READ,
            &raw mut raw,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        KeyOpen::Absent
    } else if status != ERROR_SUCCESS {
        KeyOpen::Invalid
    } else {
        KeyOpen::Open(RegKey(raw))
    }
}

/// Queries and decodes the `InstallLocation` string value.
fn query_location(key: &RegKey) -> Option<PathBuf> {
    let value_name = wide("InstallLocation");
    let mut kind = 0u32;
    let mut size = 0u32;
    // SAFETY: the key is open, the value name is NUL-terminated, and `size` is writable.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if (status != ERROR_SUCCESS && status != ERROR_MORE_DATA) || size == 0 {
        return None;
    }
    let bytes = usize::try_from(size).ok()?;
    if bytes > MAX_LOCATION_BYTES {
        return None;
    }
    let mut data = vec![0u8; bytes];
    // SAFETY: the same open key is used and `data` has `bytes` writable bytes.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            data.as_mut_ptr(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || kind != REG_SZ {
        return None;
    }
    let size = usize::try_from(size).ok()?;
    if size > data.len() || size % 2 != 0 {
        return None;
    }
    decode_location(&data[..size])
}

/// Converts the UTF-16 bytes of `InstallLocation` into a path.
fn decode_location(data: &[u8]) -> Option<PathBuf> {
    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
        .collect();
    let text = String::from_utf16(&units).ok()?;
    let text = text.trim_end_matches('\0');
    if text.is_empty() {
        return None;
    }
    Some(PathBuf::from(text))
}

/// The registry key state: absent, unreadable, or open.
enum KeyOpen {
    Absent,
    Invalid,
    Open(RegKey),
}

/// An open registry key that closes on drop.
struct RegKey(HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: this guard owns an open registry key.
        unsafe { RegCloseKey(self.0) };
    }
}

/// A NUL-terminated UTF-16 string for the registry calls.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A NUL-terminated UTF-16 path for `GetFileAttributesW`.
fn wide_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

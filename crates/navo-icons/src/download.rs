//! Verified install and removal of optional downloaded icon packs.
//!
//! Pins come from `assets/icons/sources.toml`; bytes are size- and SHA-256-checked
//! before unpacking, every SVG body is re-normalised, and outputs land atomically.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::pack::{self, Pack, PackError};
use crate::sanitise;

/// Most icons accepted from one downloaded pack.
const MAX_ENTRIES: u32 = 40_000;
/// Manifest of installed packs, read by `registry::Registry::load_downloaded`.
const MANIFEST: &str = "installed.toml";
/// Longest accepted id or version.
const MAX_NAME: usize = 64;

/// Distinguishes concurrent temporary files inside one process.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// One `[[download]]` entry pinned in `assets/icons/sources.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadSpec {
    /// The set id written into the pack and its filenames.
    pub id: String,
    /// The upstream npm package name.
    pub package: String,
    /// The pinned upstream version.
    pub version: String,
    /// The GitHub release path of the `.nvic` pack.
    pub pack_path: String,
    /// The pinned byte size of the `.nvic` pack.
    pub pack_size: u64,
    /// The pinned SHA-256 digest of the `.nvic` pack.
    pub pack_sha256: [u8; 32],
    /// The SPDX licence identifier.
    pub licence: String,
    /// The GitHub release path of the licence file.
    pub licence_path: String,
    /// The pinned byte size of the licence file.
    pub licence_size: u64,
    /// The pinned SHA-256 digest of the licence file.
    pub licence_sha256: [u8; 32],
}

/// Why a downloaded icon pack could not be installed or removed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstallError {
    /// The downloaded bytes are not the pinned length.
    #[error("expected {want} bytes, got {got}")]
    Size {
        /// The pinned byte count.
        want: u64,
        /// The received byte count.
        got: u64,
    },
    /// The downloaded bytes are not the pinned SHA-256 digest.
    #[error("SHA-256 mismatch")]
    Hash,
    /// The pack could not be opened or read.
    #[error(transparent)]
    Pack(#[from] PackError),
    /// One SVG body failed re-validation.
    #[error("icon `{name}` failed sanitisation: {error}")]
    Svg {
        /// The icon name.
        name: String,
        /// The normaliser's message.
        error: String,
    },
    /// The pack holds too many entries or does not hold exactly the pinned set.
    #[error("icon pack has too many entries")]
    TooManyEntries,
    /// A filesystem or manifest failure.
    #[error("install I/O error: {0}")]
    Io(String),
}

/// Returns the pinned optional download sets; a parse failure logs and yields no specs.
pub fn downloads() -> Vec<DownloadSpec> {
    match parse_downloads(include_str!("../../../assets/icons/sources.toml")) {
        Ok(specs) => specs,
        Err(error) => {
            tracing::error!(error = %error, "failed to parse download icon sources");
            Vec::new()
        }
    }
}

/// Verifies and installs one download pack and licence into `dir`, returning the pack path.
pub fn install(
    pack_bytes: &[u8],
    licence_bytes: &[u8],
    spec: &DownloadSpec,
    dir: &Path,
) -> Result<PathBuf, InstallError> {
    install_with(pack_bytes, licence_bytes, spec, dir, &write_atomic_default)
}

/// Installs through an injected atomic writer, so callers can supply a platform replace.
pub fn install_with(
    pack_bytes: &[u8],
    licence_bytes: &[u8],
    spec: &DownloadSpec,
    dir: &Path,
    write_atomic: &dyn Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<PathBuf, InstallError> {
    check_names(spec)?;
    check_sizes(pack_bytes, licence_bytes, spec)?;
    check_hashes(pack_bytes, licence_bytes, spec)?;
    validate_pack(pack_bytes, spec)?;
    let (entries, _) = read_manifest(dir)?;

    fs::create_dir_all(dir).map_err(|error| io_error(dir, error))?;
    let pack_name = format!("{}-{}.nvic", spec.id, spec.version);
    let pack_file = dir.join(&pack_name);
    write_file(&pack_file, pack_bytes, write_atomic)?;
    write_file(
        &dir.join(format!("{}-LICENSE", spec.id)),
        licence_bytes,
        write_atomic,
    )?;

    let digest = blake3::hash(pack_bytes).to_hex();
    let entries = upsert(entries, &spec.id, &pack_name, digest.as_str());
    write_manifest(dir, &entries, write_atomic)?;
    Ok(pack_file)
}

/// Removes an installed pack, its licence file and its `installed.toml` entry.
pub fn remove(id: &str, dir: &Path) -> Result<(), InstallError> {
    remove_with(id, dir, &write_atomic_default)
}

/// Removes through an injected atomic writer, so callers can supply a platform replace.
pub fn remove_with(
    id: &str,
    dir: &Path,
    write_atomic: &dyn Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<(), InstallError> {
    if !is_safe_component(id) {
        return Err(InstallError::Io(format!("unsafe download id `{id}`")));
    }
    let (entries, had_manifest) = read_manifest(dir)?;

    let mut kept = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.id == id {
            remove_ignoring_missing(&dir.join(&entry.file))?;
        } else {
            kept.push(entry);
        }
    }
    remove_ignoring_missing(&dir.join(format!("{id}-LICENSE")))?;
    if had_manifest {
        write_manifest(dir, &kept, write_atomic)?;
    }
    Ok(())
}

/// Refuses ids and versions that could escape the install directory.
fn check_names(spec: &DownloadSpec) -> Result<(), InstallError> {
    if !is_safe_component(&spec.id) {
        return Err(InstallError::Io(format!(
            "unsafe download id `{}`",
            spec.id
        )));
    }
    if !is_safe_component(&spec.version) {
        return Err(InstallError::Io(format!(
            "unsafe download version `{}`",
            spec.version
        )));
    }
    Ok(())
}

/// Accepts only `[a-z0-9._-]` with no `..`, so the value is one safe filename part.
fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAME
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// Checks both pinned lengths before any hashing or unpacking.
fn check_sizes(
    pack_bytes: &[u8],
    licence_bytes: &[u8],
    spec: &DownloadSpec,
) -> Result<(), InstallError> {
    let pack_len = pack_bytes.len() as u64;
    if pack_len != spec.pack_size {
        return Err(InstallError::Size {
            want: spec.pack_size,
            got: pack_len,
        });
    }
    let licence_len = licence_bytes.len() as u64;
    if licence_len != spec.licence_size {
        return Err(InstallError::Size {
            want: spec.licence_size,
            got: licence_len,
        });
    }
    Ok(())
}

/// Checks both pinned SHA-256 digests before the pack is opened.
fn check_hashes(
    pack_bytes: &[u8],
    licence_bytes: &[u8],
    spec: &DownloadSpec,
) -> Result<(), InstallError> {
    if Sha256::digest(pack_bytes).as_slice() != spec.pack_sha256.as_slice() {
        return Err(InstallError::Hash);
    }
    if Sha256::digest(licence_bytes).as_slice() != spec.licence_sha256.as_slice() {
        return Err(InstallError::Hash);
    }
    Ok(())
}

/// Requires exactly the pinned set and re-normalises every icon body.
fn validate_pack(pack_bytes: &[u8], spec: &DownloadSpec) -> Result<(), InstallError> {
    let pack = Pack::open(pack_bytes)?;
    let set = match pack.sets() {
        [set] if set.id == spec.id && set.count <= MAX_ENTRIES => set,
        _ => return Err(InstallError::TooManyEntries),
    };
    let count = usize::try_from(set.count).map_err(|_| InstallError::TooManyEntries)?;
    let names = pack.names(&set.id);
    if names.len() != count {
        return Err(InstallError::Pack(PackError::BadText));
    }

    let mut by_chunk: BTreeMap<u16, Vec<(&str, pack::IconRef)>> = BTreeMap::new();
    for name in names {
        let icon = pack
            .find(&set.id, name)
            .ok_or(InstallError::Pack(PackError::OutOfRange))?;
        by_chunk.entry(icon.chunk).or_default().push((name, icon));
    }
    for (chunk, icons) in by_chunk {
        let decoded = pack.chunk(&set.id, chunk)?;
        for (name, icon) in icons {
            validate_body(&decoded, &icon, name)?;
        }
    }
    Ok(())
}

/// Normalises one body, refusing both errors and anything the normaliser stripped.
fn validate_body(chunk: &[u8], icon: &pack::IconRef, name: &str) -> Result<(), InstallError> {
    let body = pack::body(chunk, icon).ok_or(InstallError::Pack(PackError::OutOfRange))?;
    let cleaned = sanitise::normalise(body, true).map_err(|error| InstallError::Svg {
        name: name.to_owned(),
        error: error.to_string(),
    })?;
    if cleaned.removed.is_empty() {
        return Ok(());
    }
    let labels: Vec<&str> = cleaned
        .removed
        .iter()
        .map(|removed| removed.label())
        .collect();
    Err(InstallError::Svg {
        name: name.to_owned(),
        error: format!("removed {}", labels.join(", ")),
    })
}

/// One verified entry of `installed.toml`.
struct Installed {
    id: String,
    file: String,
    blake3: String,
}

/// Reads `dir/installed.toml`; the flag says whether the file existed.
fn read_manifest(dir: &Path) -> Result<(Vec<Installed>, bool), InstallError> {
    let path = dir.join(MANIFEST);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok((Vec::new(), false)),
        Err(error) => return Err(io_error(&path, error)),
    };
    let table: toml::Table = toml::from_str(&text)
        .map_err(|error| InstallError::Io(format!("{}: {error}", path.display())))?;
    Ok((parse_manifest(&table)?, true))
}

/// Parses both `[[set]]` rows and `[sets.<id>]` tables, as the registry does.
fn parse_manifest(table: &toml::Table) -> Result<Vec<Installed>, InstallError> {
    let mut entries = Vec::new();
    if let Some(rows) = table.get("set").and_then(toml::Value::as_array) {
        for row in rows {
            let row = row.as_table().ok_or_else(|| {
                InstallError::Io("installed.toml `[[set]]` row is not a table".to_owned())
            })?;
            entries.push(parse_row(row, None)?);
        }
    }
    if let Some(rows) = table.get("sets").and_then(toml::Value::as_table) {
        for (id, row) in rows {
            let row = row.as_table().ok_or_else(|| {
                InstallError::Io(format!("installed.toml `[sets.{id}]` row is not a table"))
            })?;
            entries.push(parse_row(row, Some(id))?);
        }
    }
    Ok(entries)
}

/// Reads one manifest row, refusing an unsafe pack filename.
fn parse_row(row: &toml::Table, key: Option<&str>) -> Result<Installed, InstallError> {
    let id = key
        .map(str::to_owned)
        .or_else(|| {
            row.get("id")
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| InstallError::Io("installed.toml entry has no `id`".to_owned()))?;
    let file = text_value(row, "file")?;
    let blake3 = text_value(row, "blake3")?;
    if !is_single_filename(&file) {
        return Err(InstallError::Io(format!(
            "installed.toml entry `{id}` has an unsafe file name"
        )));
    }
    Ok(Installed { id, file, blake3 })
}

/// Reads a required string field from one manifest row.
fn text_value(row: &toml::Table, key: &str) -> Result<String, InstallError> {
    row.get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| InstallError::Io(format!("installed.toml entry has no `{key}`")))
}

/// Replaces the entry for `id` and sorts the manifest by id.
fn upsert(mut entries: Vec<Installed>, id: &str, file: &str, blake3: &str) -> Vec<Installed> {
    entries.retain(|entry| entry.id != id);
    entries.push(Installed {
        id: id.to_owned(),
        file: file.to_owned(),
        blake3: blake3.to_owned(),
    });
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    entries
}

/// Writes `[[set]]` rows in the format `registry::files` reads.
fn write_manifest(
    dir: &Path,
    entries: &[Installed],
    write_atomic: &dyn Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<(), InstallError> {
    let rows: Vec<toml::Value> = entries
        .iter()
        .map(|entry| {
            let mut row = toml::Table::new();
            row.insert("id".to_owned(), toml::Value::String(entry.id.clone()));
            row.insert("file".to_owned(), toml::Value::String(entry.file.clone()));
            row.insert(
                "blake3".to_owned(),
                toml::Value::String(entry.blake3.clone()),
            );
            toml::Value::Table(row)
        })
        .collect();
    let mut table = toml::Table::new();
    table.insert("set".to_owned(), toml::Value::Array(rows));
    let text = toml::to_string(&table)
        .map_err(|error| InstallError::Io(format!("installed.toml: {error}")))?;
    write_file(&dir.join(MANIFEST), text.as_bytes(), write_atomic)
}

/// Runs the injected writer, naming the path in any I/O error.
fn write_file(
    path: &Path,
    bytes: &[u8],
    write_atomic: &dyn Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<(), InstallError> {
    write_atomic(path, bytes).map_err(|error| io_error(path, error))
}

/// Default atomic writer: temp file in the same directory, flushed, then renamed.
fn write_atomic_default(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "path has no parent directory")
    })?;
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "path has no UTF-8 name"))?;
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(".{name}.tmp-{}-{seq}", std::process::id()));
    let result = write_temp(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Writes and flushes the temp file; `create_new` refuses symlinks and collisions.
fn write_temp(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Removes one file, treating an already-absent file as success.
fn remove_ignoring_missing(path: &Path) -> Result<(), InstallError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(path, error)),
    }
}

/// True for one plain filename: no separators, no parent component.
fn is_single_filename(name: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\', '\0']) {
        return false;
    }
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// Wraps an I/O error with the path that failed.
fn io_error(path: &Path, error: std::io::Error) -> InstallError {
    InstallError::Io(format!("{}: {error}", path.display()))
}

/// Parses the `[[download]]` array out of the sources TOML.
fn parse_downloads(text: &str) -> Result<Vec<DownloadSpec>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|error| error.to_string())?;
    let Some(rows) = table.get("download").and_then(toml::Value::as_array) else {
        return Ok(Vec::new());
    };
    rows.iter().map(parse_download).collect()
}

/// Parses one `[[download]]` table row.
fn parse_download(row: &toml::Value) -> Result<DownloadSpec, String> {
    let row = row
        .as_table()
        .ok_or_else(|| "download entry must be a table".to_owned())?;
    Ok(DownloadSpec {
        id: string_field(row, "id")?,
        package: string_field(row, "package")?,
        version: string_field(row, "version")?,
        pack_path: string_field(row, "pack_path")?,
        pack_size: u64_field(row, "pack_size")?,
        pack_sha256: sha256_field(row, "pack_sha256")?,
        licence: string_field(row, "licence")?,
        licence_path: string_field(row, "licence_path")?,
        licence_size: u64_field(row, "licence_size")?,
        licence_sha256: sha256_field(row, "licence_sha256")?,
    })
}

fn string_field(row: &toml::Table, key: &str) -> Result<String, String> {
    row.get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("download entry has no string `{key}`"))
}

fn u64_field(row: &toml::Table, key: &str) -> Result<u64, String> {
    let value = row
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| format!("download entry has no integer `{key}`"))?;
    u64::try_from(value).map_err(|_| format!("download entry `{key}` is negative"))
}

fn sha256_field(row: &toml::Table, key: &str) -> Result<[u8; 32], String> {
    let hex = row
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("download entry has no string `{key}`"))?;
    parse_sha256(hex).ok_or_else(|| format!("download entry `{key}` is not a SHA-256 hex digest"))
}

/// Decodes exactly 64 lowercase or uppercase hex digits into 32 bytes.
fn parse_sha256(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *slot = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests;

//! Validated installer bytes, package metadata, hashes and log retention policy.

use crate::placement::Space;
use navo_update::{Version, parse_version};
use sha2::{Digest, Sha256};
#[cfg(any(test, feature = "tester"))]
use std::fs;
use std::{
    borrow::Cow,
    collections::BTreeSet,
    fmt,
    io::{self, Read},
    path::{Path, PathBuf},
};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/payload.rs"));
}
#[cfg(windows)]
#[path = "payload_windows.rs"]
mod platform;
#[cfg(windows)]
pub use platform::{Extracted, Logs, Work, rotate_logs};
#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;

/// Missing or damaged payloads always map to IW17 exit 9.
#[derive(Debug)]
pub enum PayloadError {
    Missing,
    Damaged,
    Io(io::Error),
}
impl PayloadError {
    /// Returns the silent-mode code for unavailable or invalid installer bytes.
    pub fn exit_code(&self) -> u8 {
        9
    }
}
impl fmt::Display for PayloadError {
    /// Writes the IW15 message without exposing archive contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "This build carries no Navo package",
            Self::Damaged | Self::Io(_) => "This build carries a damaged Navo package",
        })
    }
}
impl std::error::Error for PayloadError {
    /// Retains an underlying read error for diagnostics when present.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Checked Setup.exe and full nupkg bytes with their release identity.
#[derive(Debug)]
pub struct Payload {
    pub setup_sha256: [u8; 32],
    pub setup_size: u64,
    pub package_sha256: [u8; 32],
    pub package_size: u64,
    pub package_name: String,
    pub version: Version,
    setup: Cow<'static, [u8]>,
    package: Cow<'static, [u8]>,
}
impl Payload {
    /// Loads embedded bytes and validates the nuspec; absent or damaged data returns exit 9.
    pub fn embedded() -> Result<Self, PayloadError> {
        if !embedded::AVAILABLE {
            return Err(PayloadError::Missing);
        }
        let payload = Self {
            setup_sha256: embedded::SETUP_SHA256,
            setup_size: embedded::SETUP_SIZE,
            package_sha256: embedded::PACKAGE_SHA256,
            package_size: embedded::PACKAGE_SIZE,
            package_name: embedded::PACKAGE_NAME.to_owned(),
            version: parse_version(env!("CARGO_PKG_VERSION")).map_err(|_| PayloadError::Damaged)?,
            setup: Cow::Borrowed(embedded::SETUP),
            package: Cow::Borrowed(embedded::PACKAGE),
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Loads tester files with the supplied version; damaged files return exit 9.
    #[cfg(any(test, feature = "tester"))]
    pub fn from_files(
        setup: &Path,
        package: &Path,
        version: Version,
    ) -> Result<Self, PayloadError> {
        let setup = fs::read(setup).map_err(PayloadError::Io)?;
        let name = package
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(PayloadError::Damaged)?;
        let package = fs::read(package).map_err(PayloadError::Io)?;
        let payload = Self {
            setup_sha256: Sha256::digest(&setup).into(),
            setup_size: setup.len() as u64,
            package_sha256: Sha256::digest(&package).into(),
            package_size: package.len() as u64,
            package_name: name.to_owned(),
            version,
            setup: Cow::Owned(setup),
            package: Cow::Owned(package),
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Returns the IW7 space need from the embedded Setup.exe and nupkg entries.
    pub fn space(&self) -> Space {
        let extracted =
            zip::ZipArchive::new(io::Cursor::new(self.package())).map_or(0, |mut zip| {
                (0..zip.len()).fold(0u64, |sum, index| {
                    sum.saturating_add(zip.by_index(index).map_or(0, |entry| entry.size()))
                })
            });
        Space {
            payload: self.setup_size.saturating_add(self.package_size),
            compressed: self.package_size,
            extracted,
        }
    }

    /// Borrows the checked full nupkg for upgrade staging.
    pub fn package(&self) -> &[u8] {
        &self.package
    }

    /// Checks both byte streams and the unique, bounded nuspec before any execution.
    fn validate(&self) -> Result<(), PayloadError> {
        if self.package_name != format!("NavoApp-{}-full.nupkg", self.version) {
            return Err(PayloadError::Damaged);
        }
        verify_hash(
            &mut self.setup.as_ref(),
            &self.setup_sha256,
            self.setup_size,
        )
        .map_err(PayloadError::Io)?;
        verify_hash(
            &mut self.package.as_ref(),
            &self.package_sha256,
            self.package_size,
        )
        .map_err(PayloadError::Io)?;
        let mut archive = zip::ZipArchive::new(io::Cursor::new(self.package()))
            .map_err(|_| PayloadError::Damaged)?;
        let mut nuspec = None;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|_| PayloadError::Damaged)?;
            if !entry
                .name()
                .map_err(|_| PayloadError::Damaged)?
                .ends_with(".nuspec")
            {
                continue;
            }
            if nuspec.is_some() || entry.size() > 64 * 1024 {
                return Err(PayloadError::Damaged);
            }
            let mut text = String::new();
            entry
                .by_ref()
                .take(64 * 1024 + 1)
                .read_to_string(&mut text)
                .map_err(|_| PayloadError::Damaged)?;
            nuspec = Some(crate::mode::parse_sq_version(&text).map_err(|_| PayloadError::Damaged)?);
        }
        let metadata = nuspec.ok_or(PayloadError::Damaged)?;
        if metadata.pack_id != "NavoApp" || metadata.version != self.version {
            return Err(PayloadError::Damaged);
        }
        Ok(())
    }
}

/// Hashes an open reader and checks its size in bytes; mismatches fail closed.
pub fn verify_hash(reader: &mut impl Read, expected: &[u8; 32], size: u64) -> io::Result<()> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    let mut count = 0_u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hash.update(&buffer[..read]);
        count += read as u64;
    }
    let actual: [u8; 32] = hash.finalize().into();
    if count != size || actual != *expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "installer payload hash or size mismatch",
        ));
    }
    Ok(())
}

/// One observed log path; reparse points never participate in retention or deletion.
#[derive(Debug)]
pub struct LogEntry {
    pub path: PathBuf,
    pub reparse: bool,
}

/// Selects files from runs older than the latest five UTC stems, retaining all companions.
pub fn logs_to_remove(entries: &[LogEntry]) -> Vec<PathBuf> {
    let stems: BTreeSet<_> = entries
        .iter()
        .filter(|entry| !entry.reparse)
        .filter_map(|entry| log_stem(&entry.path))
        .collect();
    let expired: BTreeSet<_> = stems.iter().rev().skip(5).copied().collect();
    entries
        .iter()
        .filter(|entry| !entry.reparse)
        .filter(|entry| log_stem(&entry.path).is_some_and(|stem| expired.contains(stem)))
        .map(|entry| entry.path.clone())
        .collect()
}

/// Recognizes a file-safe UTC timestamp and only the wizard/Velopack log suffixes.
fn log_stem(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let stem = ["-velopack.log.old", "-velopack.log", ".log.old", ".log"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))?;
    let bytes = stem.as_bytes();
    (bytes.len() == 19
        && bytes[8] == b'T'
        && bytes[18] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 8 || i == 18 || b.is_ascii_digit()))
    .then_some(stem)
}

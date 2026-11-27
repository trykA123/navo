//! Builds and verifies optional downloadable `NVIC1` single-set icon packs (BB-D12, BB36).

use std::path::{Component, Path};

use navo_icons::pack;
use navo_icons::sources::{self, SourceSpec};
use serde::Deserialize;

use super::{
    ReaderConfig, reader_to_source, reader_wants, sha256_hex, tarball_filename,
    validate_licence_filename, validate_sha256,
};

/// One optional downloadable icon set pinned in `assets/icons/sources.toml`.
#[derive(Debug, Deserialize)]
pub(crate) struct DownloadSource {
    /// Set identifier written into the pack and release asset filenames.
    pub(crate) id: String,
    /// Upstream npm package name.
    pub(crate) package: String,
    /// Pinned upstream version string.
    pub(crate) version: String,
    /// Canonical npm tarball URL (filename after the last `/` is read from `--tarballs`).
    pub(crate) url: String,
    /// Expected lowercase SHA-256 hex digest of the npm tarball.
    pub(crate) sha256: String,
    /// Expected byte size of the npm tarball.
    pub(crate) size: u64,
    /// GitHub release download path for the `.nvic` pack.
    pub(crate) pack_path: String,
    /// Expected byte size of the `.nvic` pack.
    pub(crate) pack_size: u64,
    /// Expected lowercase SHA-256 hex digest of the `.nvic` pack.
    pub(crate) pack_sha256: String,
    /// SPDX licence identifier written into the set record.
    pub(crate) licence: String,
    /// Path to the licence file inside the tarball (mutually exclusive with `licence_local`).
    #[serde(default)]
    pub(crate) licence_file: Option<String>,
    /// Workspace-relative path to a checked-in licence file when absent from the tarball.
    #[serde(default)]
    pub(crate) licence_local: Option<String>,
    /// GitHub release download path for the `<id>-LICENSE` file.
    pub(crate) licence_path: String,
    /// Expected byte size of the licence file.
    pub(crate) licence_size: u64,
    /// Expected lowercase SHA-256 hex digest of the licence file.
    pub(crate) licence_sha256: String,
    /// Icon grid size in pixels.
    pub(crate) grid: u16,
    /// Reader layout inside the tarball.
    #[serde(flatten)]
    pub(crate) reader: ReaderConfig,
}

struct BuiltDownload {
    pack_bytes: Vec<u8>,
    pack_sha256: String,
    licence_bytes: Vec<u8>,
    licence_sha256: String,
    icon_count: usize,
    skipped: Vec<String>,
}

/// Validates required invariants on one `[[download]]` entry.
pub(crate) fn validate_download(dl: &DownloadSource) -> Result<(), String> {
    validate_licence_filename(&format!("{}-LICENSE", dl.id))?;
    validate_sha256(&dl.sha256, &dl.id)?;
    validate_sha256(&dl.pack_sha256, &dl.id)?;
    validate_sha256(&dl.licence_sha256, &dl.id)?;
    tarball_filename(&dl.url)?;
    if !dl.pack_path.starts_with('/') || !dl.licence_path.starts_with('/') {
        return Err(format!(
            "download `{}` pack_path and licence_path must start with `/`",
            dl.id
        ));
    }
    match (&dl.licence_file, &dl.licence_local) {
        (Some(file), None) if !file.is_empty() => Ok(()),
        (None, Some(local)) if !local.is_empty() => validate_local_path(local),
        (Some(_), Some(_)) => Err(format!(
            "download `{}` must not set both `licence_file` and `licence_local`",
            dl.id
        )),
        _ => Err(format!(
            "download `{}` must set either `licence_file` or `licence_local`",
            dl.id
        )),
    }
}

/// Ensures a `licence_local` path contains no `..` traversal components.
fn validate_local_path(path: &str) -> Result<(), String> {
    if Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "invalid licence_local path `{path}`: parent traversal is not allowed"
        ));
    }
    Ok(())
}

/// Builds every `[[download]]` pack and licence file into `out_dir`.
pub(crate) fn pack_downloads(
    downloads: &[DownloadSource],
    tarballs: &Path,
    out_dir: &Path,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("cannot create {}: {error}", out_dir.display()))?;
    let mut lines = Vec::with_capacity(downloads.len());
    for dl in downloads {
        let built = build_download(dl, tarballs)?;
        log_skipped(&dl.id, &built.skipped);
        let pack_file = out_dir.join(format!("{}.nvic", dl.id));
        std::fs::write(&pack_file, &built.pack_bytes)
            .map_err(|error| format!("cannot write {}: {error}", pack_file.display()))?;
        let licence_file = out_dir.join(format!("{}-LICENSE", dl.id));
        std::fs::write(&licence_file, &built.licence_bytes)
            .map_err(|error| format!("cannot write {}: {error}", licence_file.display()))?;
        lines.push(format!(
            "{}: {} icons, pack={} ({} bytes), licence={} ({} bytes), skipped={}",
            dl.id,
            built.icon_count,
            built.pack_sha256,
            built.pack_bytes.len(),
            built.licence_sha256,
            built.licence_bytes.len(),
            built.skipped.len(),
        ));
    }
    Ok(lines)
}

/// Rebuilds every `[[download]]` set and verifies pack and licence hashes and sizes.
pub(crate) fn verify_downloads(
    downloads: &[DownloadSource],
    tarballs: &Path,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::with_capacity(downloads.len());
    for dl in downloads {
        let built = build_download(dl, tarballs)?;
        log_skipped(&dl.id, &built.skipped);
        verify_digest(
            &dl.id,
            "pack",
            (built.pack_bytes.len() as u64, &built.pack_sha256),
            (dl.pack_size, &dl.pack_sha256),
        )?;
        verify_digest(
            &dl.id,
            "licence",
            (built.licence_bytes.len() as u64, &built.licence_sha256),
            (dl.licence_size, &dl.licence_sha256),
        )?;
        lines.push(format!(
            "{} matches pack={} ({} bytes) licence={} ({} bytes)",
            dl.id,
            built.pack_sha256,
            built.pack_bytes.len(),
            built.licence_sha256,
            built.licence_bytes.len(),
        ));
    }
    Ok(lines)
}

/// Verifies tarball SHA-256 and size, extracts the licence, and builds one single-set pack.
fn build_download(dl: &DownloadSource, tarballs: &Path) -> Result<BuiltDownload, String> {
    let filename = tarball_filename(&dl.url)?;
    let path = tarballs.join(filename);
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("cannot read tarball {}: {error}", path.display()))?;
    verify_digest(
        &dl.id,
        "tarball",
        (bytes.len() as u64, &sha256_hex(&bytes)),
        (dl.size, &dl.sha256),
    )?;
    let files = sources::read_tarball(&bytes, &|entry| {
        dl.licence_file.as_deref() == Some(entry) || reader_wants(&dl.reader, entry)
    })
    .map_err(|error| format!("cannot read tarball {}: {error}", path.display()))?;
    let licence_bytes = read_licence(dl, &files, filename)?;
    let licence_sha256 = sha256_hex(&licence_bytes);
    let spec = SourceSpec {
        id: dl.id.clone(),
        version: dl.version.clone(),
        licence: dl.licence.clone(),
        grid: dl.grid,
        reader: reader_to_source(&dl.reader),
    };
    let (draft, skipped) = sources::build_set_lenient(&files, &spec)
        .map_err(|error| format!("cannot build set `{}`: {error}", dl.id))?;
    let icon_count = draft.icons.len();
    let pack_bytes = pack::write(std::slice::from_ref(&draft))
        .map_err(|error| format!("cannot pack `{}`: {error}", dl.id))?;
    let pack_sha256 = sha256_hex(&pack_bytes);
    Ok(BuiltDownload {
        pack_bytes,
        pack_sha256,
        licence_bytes,
        licence_sha256,
        icon_count,
        skipped,
    })
}

/// Reads the licence bytes either from the tarball (`licence_file`) or disk (`licence_local`).
fn read_licence(
    dl: &DownloadSource,
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    filename: &str,
) -> Result<Vec<u8>, String> {
    if let Some(in_tar) = dl.licence_file.as_deref() {
        return files.get(in_tar).cloned().ok_or_else(|| {
            format!(
                "missing licence file `{in_tar}` in {} ({filename})",
                dl.package
            )
        });
    }
    let local = dl
        .licence_local
        .as_deref()
        .ok_or_else(|| format!("download `{}` has no licence source", dl.id))?;
    std::fs::read(local).map_err(|error| format!("cannot read licence_local `{local}`: {error}"))
}

/// Checks byte size and lowercase SHA-256 digest against expected values.
fn verify_digest(
    id: &str,
    label: &str,
    actual: (u64, &str),
    expected: (u64, &str),
) -> Result<(), String> {
    if actual.0 != expected.0 {
        return Err(format!(
            "{label} size mismatch for `{id}`: expected {}, got {}",
            expected.0, actual.0
        ));
    }
    let expected_lower = expected.1.to_ascii_lowercase();
    if actual.1 != expected_lower {
        return Err(format!(
            "{label} sha256 mismatch for `{id}`: expected {expected_lower}, got {}",
            actual.1
        ));
    }
    Ok(())
}

/// Logs any skipped icon names for a download set to stderr.
fn log_skipped(id: &str, skipped: &[String]) {
    if !skipped.is_empty() {
        eprintln!(
            "xtask icons: `{id}` skipped {} icon(s): {}",
            skipped.len(),
            skipped.join(", ")
        );
    }
}

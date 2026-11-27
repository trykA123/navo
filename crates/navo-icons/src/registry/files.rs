//! File-backed icon resolution and verified loading of downloaded icon packs.

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use super::{DownloadedPack, Resolved};
use crate::io::{self, ReadError};
use crate::pack::Pack;
use crate::raster;
use crate::sanitise;

/// Maximum size accepted for `installed.toml` (1 MiB).
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Maximum size accepted for a downloaded `.nvic` pack (32 MiB).
const MAX_DOWNLOAD_PACK_BYTES: u64 = 32 * 1024 * 1024;

struct ManifestEntry {
    id: String,
    file: String,
    blake3: String,
}

/// Reads `dir/installed.toml` and verifies every listed `.nvic` pack by BLAKE3 and header.
pub(super) fn load_manifest_packs(dir: &Path) -> (Option<Vec<DownloadedPack>>, Vec<String>) {
    let manifest_path = dir.join("installed.toml");
    let bytes = match io::read_capped(&manifest_path, MAX_MANIFEST_BYTES) {
        Ok(bytes) => bytes,
        Err(ReadError::Io(err)) if err.kind() == ErrorKind::NotFound => {
            return (Some(Vec::new()), Vec::new());
        }
        Err(err) => return (None, vec![format!("failed to read installed.toml: {err}")]),
    };

    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(err) => {
            return (
                None,
                vec![format!("installed.toml is not valid UTF-8: {err}")],
            );
        }
    };
    let table: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(err) => return (None, vec![format!("failed to parse installed.toml: {err}")]),
    };

    let mut warnings = Vec::new();
    let entries = parse_manifest_entries(&table, &mut warnings);
    let mut loaded = Vec::with_capacity(entries.len());

    for entry in entries {
        match load_verified_pack(dir, &entry) {
            Ok(pack) => loaded.push(pack),
            Err(warning) => warnings.push(warning),
        }
    }

    (Some(loaded), warnings)
}

/// Resolves an `IconSpec::Image` file after grammar validation and full re-validation.
pub(super) fn resolve_image(file: &str, icons_dir: &Path) -> (Resolved, Option<String>) {
    if is_valid_icon_file(file, ".png") {
        let path = icons_dir.join(file);
        let bytes = match io::read_capped(&path, raster::RASTER_CAP) {
            Ok(bytes) => bytes,
            Err(err) => {
                return (
                    Resolved::Missing { dashed: false },
                    Some(format!("failed to read icon `{file}`: {err}")),
                );
            }
        };
        return match raster::reencode(&bytes) {
            Ok(png) => (
                Resolved::Raster {
                    png: Arc::from(png),
                },
                None,
            ),
            Err(err) => (
                Resolved::Missing { dashed: false },
                Some(format!("invalid raster icon `{file}`: {err}")),
            ),
        };
    }

    if is_valid_icon_file(file, ".svg") {
        return resolve_svg_bytes(file, false, icons_dir);
    }

    (
        Resolved::Missing { dashed: false },
        Some(format!("invalid icon filename `{file}`")),
    )
}

/// Resolves an `IconSpec::Svg` file after grammar validation and SVG normalisation.
pub(super) fn resolve_svg(file: &str, mono: bool, icons_dir: &Path) -> (Resolved, Option<String>) {
    if !is_valid_icon_file(file, ".svg") {
        return (
            Resolved::Missing { dashed: false },
            Some(format!("invalid icon filename `{file}`")),
        );
    }
    resolve_svg_bytes(file, mono, icons_dir)
}

fn resolve_svg_bytes(file: &str, mono: bool, icons_dir: &Path) -> (Resolved, Option<String>) {
    let path = icons_dir.join(file);
    let bytes = match io::read_capped(&path, sanitise::SVG_CAP as u64) {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                Resolved::Missing { dashed: false },
                Some(format!("failed to read icon `{file}`: {err}")),
            );
        }
    };
    let cleaned = match sanitise::normalise(&bytes, mono) {
        Ok(cleaned) => cleaned,
        Err(err) => {
            return (
                Resolved::Missing { dashed: false },
                Some(format!("invalid SVG icon `{file}`: {err}")),
            );
        }
    };
    let svg = Arc::from(cleaned.svg);
    if mono {
        (
            Resolved::Mono {
                svg,
                tone: None,
                brand: None,
            },
            None,
        )
    } else {
        (Resolved::Colour { svg }, None)
    }
}

/// Checks the BB-D5 stored icon filename grammar: `^[a-z0-9-]{1,32}-[0-9a-f]{8}<ext>$`.
fn is_valid_icon_file(file: &str, ext: &str) -> bool {
    if file.len() > 45 {
        return false;
    }
    let Some(stem_and_hash) = file.strip_suffix(ext) else {
        return false;
    };
    let Some((stem, hash)) = stem_and_hash.rsplit_once('-') else {
        return false;
    };
    let stem_ok = !stem.is_empty()
        && stem.len() <= 32
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    let hash_ok = hash.len() == 8 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    stem_ok && hash_ok
}

/// Parses `[[set]]` array entries or `[sets.<id>]` table entries from `installed.toml`.
fn parse_manifest_entries(table: &toml::Table, warnings: &mut Vec<String>) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    if let Some(list) = table.get("set").and_then(toml::Value::as_array) {
        for item in list {
            let Some(row) = item.as_table() else {
                warnings.push("installed.toml `[[set]]` entry must be a table".to_owned());
                continue;
            };
            push_manifest_row(row, None, &mut entries, warnings);
        }
    }
    if let Some(sets) = table.get("sets").and_then(toml::Value::as_table) {
        for (id, item) in sets {
            let Some(row) = item.as_table() else {
                warnings.push(format!(
                    "installed.toml `[sets.{id}]` entry must be a table"
                ));
                continue;
            };
            push_manifest_row(row, Some(id), &mut entries, warnings);
        }
    }
    entries
}

fn push_manifest_row(
    row: &toml::Table,
    key_id: Option<&str>,
    out: &mut Vec<ManifestEntry>,
    warnings: &mut Vec<String>,
) {
    let id = match key_id.or_else(|| row.get("id").and_then(toml::Value::as_str)) {
        Some(id) if !id.is_empty() => id.to_owned(),
        _ => {
            warnings.push("installed.toml entry is missing `id`".to_owned());
            return;
        }
    };
    let Some(file) = row.get("file").and_then(toml::Value::as_str) else {
        warnings.push(format!("installed.toml entry `{id}` is missing `file`"));
        return;
    };
    let Some(blake3) = row.get("blake3").and_then(toml::Value::as_str) else {
        warnings.push(format!("installed.toml entry `{id}` is missing `blake3`"));
        return;
    };
    out.push(ManifestEntry {
        id,
        file: file.to_owned(),
        blake3: blake3.to_owned(),
    });
}

/// Verifies filename safety, BLAKE3 digest, and `NVIC1` header for one downloaded pack.
fn load_verified_pack(dir: &Path, entry: &ManifestEntry) -> Result<DownloadedPack, String> {
    if !io::is_pack_filename(&entry.file) {
        return Err(format!(
            "downloaded pack `{}` has invalid filename `{}`",
            entry.id, entry.file
        ));
    }
    if !is_blake3_hex(&entry.blake3) {
        return Err(format!(
            "downloaded pack `{}` has invalid blake3 digest",
            entry.id
        ));
    }
    let path = dir.join(&entry.file);
    let bytes = io::read_capped(&path, MAX_DOWNLOAD_PACK_BYTES).map_err(|err| {
        format!(
            "failed to read downloaded pack `{}` (`{}`): {err}",
            entry.id, entry.file
        )
    })?;
    let actual = blake3::hash(&bytes).to_hex();
    if !actual.as_str().eq_ignore_ascii_case(&entry.blake3) {
        return Err(format!(
            "downloaded pack `{}` failed BLAKE3 verification",
            entry.id
        ));
    }
    let pack = Pack::open(&bytes)
        .map_err(|err| format!("downloaded pack `{}` is invalid: {err}", entry.id))?;
    if !matches!(pack.sets(), [info] if info.id == entry.id) {
        return Err(format!(
            "downloaded pack `{}` does not contain set `{}`",
            entry.file, entry.id
        ));
    }
    Ok(DownloadedPack {
        sets: pack.sets().to_vec(),
        bytes: Arc::from(bytes),
    })
}

fn is_blake3_hex(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

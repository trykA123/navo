//! Imports user SVG and raster files into the button bar icon folder.
//! Enforces local-path, no-symlink, capped-read, and atomic-write rules.

use std::fs;
use std::path::Path;

use navo_core::{Location, ProviderPath};
use navo_icons::io::read_capped;
use navo_icons::raster::{RASTER_CAP, reencode, stored_name};
use navo_icons::sanitise::{SVG_CAP, normalise};
use navo_icons::spec::IconSpec;
use navo_store::replace_atomic;
use navo_ui::window_view::ImportedIcon;

/// Imports a local SVG or raster file into `icons_dir` and returns its icon spec.
pub(crate) fn import(path: &Path, icons_dir: &Path, mono: bool) -> Result<ImportedIcon, String> {
    if !is_local_path(path) {
        return Err("icon imports must come from a local path".to_owned());
    }
    let meta = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Err("icon imports must be regular files, not links or folders".to_owned());
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("icon");
    let svg = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    if svg {
        import_svg(path, icons_dir, mono, stem)
    } else {
        import_raster(path, icons_dir, stem)
    }
}

/// Rejects network paths and any address that is not a local filesystem path.
fn is_local_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\") || text.starts_with("//") {
        return false;
    }
    Location::local(ProviderPath::from_text(&text)).is_ok_and(|location| location.is_local())
}

/// Normalises, hashes, and stores an SVG icon.
fn import_svg(
    path: &Path,
    icons_dir: &Path,
    mono: bool,
    stem: &str,
) -> Result<ImportedIcon, String> {
    let bytes = read_capped(path, SVG_CAP as u64).map_err(|err| err.to_string())?;
    let cleaned = normalise(&bytes, mono).map_err(|err| err.to_string())?;
    let file = stored_name(stem, &cleaned.svg, "svg");
    store(icons_dir, &file, &cleaned.svg)?;
    Ok(ImportedIcon {
        spec: IconSpec::Svg { file, mono },
        removed: cleaned
            .removed
            .into_iter()
            .map(|feature| feature.label().to_owned())
            .collect(),
    })
}

/// Re-encodes, hashes, and stores a raster icon as a clean PNG.
fn import_raster(path: &Path, icons_dir: &Path, stem: &str) -> Result<ImportedIcon, String> {
    let bytes = read_capped(path, RASTER_CAP).map_err(|err| err.to_string())?;
    let png = reencode(&bytes).map_err(|err| err.to_string())?;
    let file = stored_name(stem, &png, "png");
    store(icons_dir, &file, &png)?;
    Ok(ImportedIcon {
        spec: IconSpec::Image { file },
        removed: Vec::new(),
    })
}

/// Writes `bytes` under `icons_dir/file`, reusing an already identical stored copy.
fn store(icons_dir: &Path, file: &str, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(icons_dir).map_err(|err| err.to_string())?;
    let target = icons_dir.join(file);
    if fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("the stored icon path is a link".to_owned());
    }
    if read_capped(&target, bytes.len() as u64).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    replace_atomic(&target, bytes).map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;

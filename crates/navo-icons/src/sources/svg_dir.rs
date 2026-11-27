//! Builds icon drafts from a tarball directory holding one SVG per icon.

use std::collections::BTreeMap;

use super::{SourceError, clean};
use crate::pack::IconDraft;

/// Builds every matching top-level file below `dir`, stripping `suffix` and skipping `skip_suffix`.
pub(super) fn build(
    files: &BTreeMap<String, Vec<u8>>,
    dir: &str,
    suffix: &str,
    strip: &str,
    skip_suffix: Option<&str>,
) -> Result<Vec<IconDraft>, SourceError> {
    let mut icons = Vec::new();
    for (name, bytes) in matching_entries(files, dir, suffix, strip, skip_suffix) {
        icons.push(IconDraft {
            svg: clean(&name, bytes)?,
            name,
            tags: Vec::new(),
            brand: None,
        });
    }
    Ok(icons)
}

/// Builds matching top-level files for download sets, skipping invalid names and broken SVGs.
pub(super) fn build_lenient(
    files: &BTreeMap<String, Vec<u8>>,
    dir: &str,
    suffix: &str,
    strip: &str,
    skip_suffix: Option<&str>,
) -> (Vec<IconDraft>, Vec<String>) {
    let mut icons = Vec::new();
    let mut skipped = Vec::new();
    for (name, bytes) in matching_entries(files, dir, suffix, strip, skip_suffix) {
        if !is_valid_icon_name(&name) {
            skipped.push(name);
            continue;
        }
        match clean(&name, bytes) {
            Ok(svg) => icons.push(IconDraft {
                name,
                svg,
                tags: Vec::new(),
                brand: None,
            }),
            Err(_) => skipped.push(name),
        }
    }
    (icons, skipped)
}

/// Yields `(normalised_name, bytes)` for direct children of `dir` matching `suffix` and not `skip_suffix`.
fn matching_entries<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    dir: &'a str,
    suffix: &'a str,
    strip: &'a str,
    skip_suffix: Option<&'a str>,
) -> impl Iterator<Item = (String, &'a [u8])> {
    files.iter().filter_map(move |(path, bytes)| {
        let rest = under(path, dir)?;
        if rest.contains('/') || !rest.ends_with(suffix) {
            return None;
        }
        if skip_suffix.is_some_and(|skip| rest.ends_with(skip)) {
            return None;
        }
        let stem = &rest[..rest.len() - suffix.len()];
        let name = if strip == "underscore" {
            stem.replace('_', "-")
        } else {
            stem.to_owned()
        }
        .to_ascii_lowercase();
        Some((name, bytes.as_slice()))
    })
}

/// Checks BB-D5 icon name rules: `^[a-z0-9][a-z0-9_-]{0,63}$`.
fn is_valid_icon_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    if bytes.len() > 64 || !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes[1..].iter().all(|&byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
    })
}

/// Returns the path below `dir`, requiring a `/` boundary after the prefix.
fn under<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

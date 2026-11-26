//! Builds icon drafts from a tarball directory holding one SVG per icon.

use std::collections::BTreeMap;

use super::{SourceError, clean};
use crate::pack::IconDraft;

/// Builds every matching file below `dir`, stripping `suffix` and skipping `skip_suffix`.
pub(super) fn build(
    files: &BTreeMap<String, Vec<u8>>,
    dir: &str,
    suffix: &str,
    strip: &str,
    skip_suffix: Option<&str>,
) -> Result<Vec<IconDraft>, SourceError> {
    let mut icons = Vec::new();
    for (path, bytes) in files {
        let Some(rest) = under(path, dir) else {
            continue;
        };
        if !rest.ends_with(suffix) {
            continue;
        }
        if skip_suffix.is_some_and(|skip| rest.ends_with(skip)) {
            continue;
        }
        let stem = &rest[..rest.len() - suffix.len()];
        let name = if strip == "underscore" {
            stem.replace('_', "-")
        } else {
            stem.to_owned()
        }
        .to_ascii_lowercase();
        icons.push(IconDraft {
            name: name.clone(),
            svg: clean(&name, bytes)?,
            tags: Vec::new(),
            brand: None,
        });
    }
    Ok(icons)
}

/// Returns the path below `dir`, requiring a `/` boundary after the prefix.
fn under<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

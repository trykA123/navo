#[cfg(any(windows, test))]
use std::path::PathBuf;

use navo_core::NameError;
#[cfg(any(windows, test))]
use navo_core::check_name;

/// Why a descriptor name was refused; the item is skipped and nothing is written for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadName {
    #[error("The name is not valid Unicode")]
    NotUnicode,
    #[error("The name is empty")]
    Empty,
    #[error("The name is an absolute path")]
    Absolute,
    #[error("The name points to a drive")]
    Drive,
    #[error("The name points to an alternate data stream")]
    Stream,
    #[error("The name leaves the target folder")]
    Traversal,
    #[error("The name has an empty folder part")]
    EmptySegment,
    #[error("{0}")]
    Segment(NameError),
}

/// Turns a descriptor name (folders split by `\`) into a relative path that stays below the
/// target folder.
#[cfg(any(windows, test))]
pub(super) fn relative_path(name: &str) -> Result<PathBuf, BadName> {
    if name.is_empty() {
        return Err(BadName::Empty);
    }
    if name.starts_with(['\\', '/']) {
        return Err(BadName::Absolute);
    }
    let mut chars = name.chars();
    if chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':') {
        return Err(BadName::Drive);
    }
    if name.contains(':') {
        return Err(BadName::Stream);
    }
    let mut path = PathBuf::new();
    for segment in name.split('\\') {
        path.push(segment_of(segment)?);
    }
    Ok(path)
}

#[cfg(any(windows, test))]
fn segment_of(segment: &str) -> Result<&str, BadName> {
    match segment {
        "" => Err(BadName::EmptySegment),
        ".." => Err(BadName::Traversal),
        _ => check_name(segment)
            .map(|()| segment)
            .map_err(BadName::Segment),
    }
}
